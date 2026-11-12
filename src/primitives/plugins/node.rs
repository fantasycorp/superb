//! [`PluginNode`]: a plugin instance as a graph node.

use crate::primitives::buffers::PlanarBuffer;
use crate::primitives::graph::{Node, NodeChannels, ProcessContext, ProcessStatus};
use crate::primitives::realtime::spsc::{self, Consumer, Producer};
use crate::primitives::SampleRate;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use core::any::Any;

use crate::primitives::plugins::{AudioPortLayout, ParamChange, PluginChange, PluginInstance};

/// Wraps a [`PluginInstance`] so it can be placed in an
/// [`AudioGraph`](crate::primitives::graph::AudioGraph).
///
/// `prepare` activates the plugin (a failure makes the node silent rather
/// than panicking), `process` first drains the parameter queue into the
/// instance and then renders. Take the queue's sending half with
/// [`take_param_sender`](PluginNode::take_param_sender) before handing the
/// node to the graph. What the plugin changes itself (a control turned in
/// its own view) comes back the other way: take
/// [`take_change_receiver`](PluginNode::take_change_receiver) too.
///
/// A plugin outlives the graph it sits in. Give the node a key with
/// [`carried_as`](PluginNode::carried_as); when a new graph replaces this
/// one, a node made with [`adopting`](PluginNode::adopting) under the same
/// key takes the running instance over at the swap, between two blocks,
/// with its state and its tails ([`Node::carry`]). Until an instance
/// arrives, an adopting node passes its input through.
///
/// ```
/// use superb::primitives::buffers::PlanarBuffer;
/// use superb::primitives::graph::{BlockTime, Node, ProcessContext, ProcessStatus};
/// use superb::primitives::midi::MidiEventBuffer;
/// use superb::primitives::plugins::{
///     AudioPortLayout, ParamChange, ParamInfo, PluginDescriptor, PluginFormat, PluginInstance,
///     PluginNode,
/// };
/// use superb::primitives::{Result, SampleRate};
///
/// struct Gain {
///     gain: f64,
///     active: bool,
///     desc: PluginDescriptor,
///     params: Vec<ParamInfo>,
/// }
/// impl PluginInstance for Gain {
///     fn descriptor(&self) -> &PluginDescriptor {
///         &self.desc
///     }
///     fn ports(&self) -> AudioPortLayout {
///         AudioPortLayout {
///             inputs: 1,
///             outputs: 1,
///         }
///     }
///     fn params(&self) -> &[ParamInfo] {
///         &self.params
///     }
///     fn param_value(&self, id: u32) -> Result<f64> {
///         if id == 1 {
///             Ok(self.gain)
///         } else {
///             Err(superb::primitives::SuperbError::NotFound {
///                 kind: "param",
///                 id: id as u64,
///             })
///         }
///     }
///     fn set_param(&mut self, id: u32, value: f64) -> Result<()> {
///         if id == 1 {
///             self.gain = value;
///             Ok(())
///         } else {
///             Err(superb::primitives::SuperbError::NotFound {
///                 kind: "param",
///                 id: id as u64,
///             })
///         }
///     }
///     fn activate(&mut self, _: SampleRate, _: usize) -> Result<()> {
///         self.active = true;
///         Ok(())
///     }
///     fn deactivate(&mut self) {
///         self.active = false;
///     }
///     fn is_active(&self) -> bool {
///         self.active
///     }
///     fn reset(&mut self) {}
///     fn process(
///         &mut self,
///         _: &ProcessContext<'_>,
///         input: &PlanarBuffer,
///         output: &mut PlanarBuffer,
///     ) -> ProcessStatus {
///         output.copy_from(input).unwrap();
///         output.apply_gain(self.gain as f32);
///         ProcessStatus::Normal
///     }
///     fn latency_frames(&self) -> usize {
///         0
///     }
///     fn save_state(&mut self) -> Result<Vec<u8>> {
///         Ok(Vec::new())
///     }
///     fn load_state(&mut self, _: &[u8]) -> Result<()> {
///         Ok(())
///     }
/// }
///
/// let instance = Gain {
///     gain: 1.0,
///     active: false,
///     desc: PluginDescriptor::new(PluginFormat::Clap, "superb.mock.gain", "Mock Gain"),
///     params: vec![ParamInfo {
///         id: 1,
///         name: "Gain".into(),
///         module: String::new(),
///         min: 0.0,
///         max: 2.0,
///         default: 1.0,
///         automatable: true,
///         stepped: false,
///         hidden: false,
///         readonly: false,
///         bypass: false,
///     }],
/// };
/// let mut node = PluginNode::new(Box::new(instance));
/// assert_eq!(node.name(), "Mock Gain");
/// let mut params = node.take_param_sender().unwrap();
/// node.prepare(SampleRate::HZ_48K, 8);
/// let midi = MidiEventBuffer::with_capacity(0);
/// let ctx = ProcessContext {
///     sample_rate: SampleRate::HZ_48K,
///     frames: 8,
///     time: BlockTime::free_running(),
///     midi: &midi,
///     input_is_silent: false,
///     input: None,
/// };
/// let mut input = PlanarBuffer::new(1, 8);
/// input.fill(1.0);
/// let mut output = PlanarBuffer::new(1, 8);
/// params.push(ParamChange::new(1, 0.5)).unwrap();
/// assert_eq!(node.process(&ctx, &input, &mut output), ProcessStatus::Normal);
/// assert_eq!(output.channel(0), &[0.5; 8]);
/// ```
pub struct PluginNode {
    /// The instance, as a `Box<Held>`. Kept behind `Any` so the graph can
    /// move it to another node at a swap without knowing what it is.
    held: Option<Box<dyn Any + Send>>,
    key: Option<u64>,
    name: String,
    channels: NodeChannels,
    /// The latency to report while no instance is held.
    latency: usize,
    /// The instance's latency after the last `prepare`, for the control side.
    measured: Arc<AtomicUsize>,
    params_rx: Consumer<ParamChange>,
    params_tx: Option<Producer<ParamChange>>,
    bypass: Arc<AtomicBool>,
    /// The receiving half of the held instance's change queue, until taken.
    changes_rx: Option<Consumer<PluginChange>>,
}

/// What a [`PluginNode`] holds and hands over at a graph swap: the
/// instance, and the queue its own changes go out on. The queue travels
/// with the instance so its receiver stays the same for the instance's
/// whole life, whichever graph it runs in.
struct Held {
    instance: Box<dyn PluginInstance>,
    changes: Producer<PluginChange>,
}

impl PluginNode {
    /// Capacity of the parameter queue (changes per block before drops).
    pub const PARAM_QUEUE_CAPACITY: usize = 256;

    /// Capacity of the queue of the plugin's own changes (changes the
    /// receiver may leave unread before new ones are dropped).
    pub const CHANGE_QUEUE_CAPACITY: usize = 1024;

    /// Wraps an instance. The node's channel layout is the plugin's main
    /// input/output port layout.
    pub fn new(instance: Box<dyn PluginInstance>) -> Self {
        let ports = instance.ports();
        let name = instance.descriptor().name.clone();
        let latency = instance.latency_frames();
        let mut node = Self::adopting(0, name, ports, latency);
        let (changes, changes_rx) = spsc::channel(Self::CHANGE_QUEUE_CAPACITY);
        node.key = None;
        node.held = Some(Box::new(Held { instance, changes }));
        node.changes_rx = Some(changes_rx);
        node
    }

    /// A node with no instance yet, which takes over the instance of the
    /// node carried under `key` in the graph it replaces. `name`, `ports`
    /// and `latency` are what that instance reports; the graph is laid out
    /// with them before the instance arrives.
    pub fn adopting(
        key: u64,
        name: impl Into<String>,
        ports: AudioPortLayout,
        latency: usize,
    ) -> Self {
        let (tx, rx) = spsc::channel(Self::PARAM_QUEUE_CAPACITY);
        Self {
            held: None,
            key: Some(key),
            name: name.into(),
            channels: NodeChannels {
                inputs: ports.inputs,
                outputs: ports.outputs,
            },
            latency,
            measured: Arc::new(AtomicUsize::new(latency)),
            params_rx: rx,
            params_tx: Some(tx),
            bypass: Arc::new(AtomicBool::new(false)),
            changes_rx: None,
        }
    }

    /// Names the instance so the node that replaces this one in the next
    /// graph can take it over (builder style). Keys are the caller's: one
    /// per plugin instance, the same across rebuilds.
    pub fn carried_as(mut self, key: u64) -> Self {
        self.key = Some(key);
        self
    }

    fn held(&self) -> Option<&Held> {
        self.held.as_ref()?.downcast_ref::<Held>()
    }

    fn held_mut(&mut self) -> Option<&mut Held> {
        self.held.as_mut()?.downcast_mut::<Held>()
    }

    /// The bypass switch, shared with the control thread. While set, the
    /// node copies its input to its output and the plugin is not run (its
    /// state freezes; parameter changes still queue up and apply when the
    /// bypass lifts). Reported latency does not change, so the signal
    /// arrives `latency_frames` early while bypassed.
    pub fn bypass_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.bypass)
    }

    /// The latency the instance reported after the last `prepare`, shared
    /// with the control thread: what to give [`adopting`](Self::adopting)
    /// when the next graph is built, so that graph is laid out with the
    /// latency the instance really has.
    pub fn latency_handle(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.measured)
    }

    /// `true` while the bypass switch is set.
    pub fn is_bypassed(&self) -> bool {
        self.bypass.load(Ordering::Relaxed)
    }

    /// The sending half of the parameter queue (once). Pushing a
    /// [`ParamChange`] is wait-free; the node hands it to the plugin before
    /// the next block, to land at its [`frame`](ParamChange::frame).
    pub fn take_param_sender(&mut self) -> Option<Producer<ParamChange>> {
        self.params_tx.take()
    }

    /// The receiving half of the queue of the plugin's own changes (once):
    /// each [`PluginChange`] the instance reports, in order, after the
    /// block it happened in. The queue belongs to the instance, not to
    /// this node: it keeps delivering after the instance has been taken
    /// over by a node of the next graph. `None` for a node made with
    /// [`adopting`](Self::adopting), which gets its instance, queue
    /// included, from the node it replaces.
    pub fn take_change_receiver(&mut self) -> Option<Consumer<PluginChange>> {
        self.changes_rx.take()
    }

    /// The wrapped instance; `None` for an adopting node nothing has
    /// reached yet, and for a node whose instance has moved on.
    pub fn instance(&self) -> Option<&dyn PluginInstance> {
        self.held().map(|h| h.instance.as_ref())
    }

    /// Mutable access to the wrapped instance (main thread, e.g. state I/O).
    pub fn instance_mut(&mut self) -> Option<&mut (dyn PluginInstance + 'static)> {
        self.held_mut().map(|h| h.instance.as_mut())
    }

    /// Unwraps the instance, if the node holds one.
    pub fn into_instance(self) -> Option<Box<dyn PluginInstance>> {
        let held = *self.held?.downcast::<Held>().ok()?;
        Some(held.instance)
    }

    /// `true` while the node holds an instance that is active: the last
    /// `prepare` activated it, here or in the node it was taken over from.
    pub fn is_prepared(&self) -> bool {
        self.held().is_some_and(|h| h.instance.is_active())
    }
}

impl Node for PluginNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn channels(&self) -> NodeChannels {
        self.channels
    }

    fn prepare(&mut self, sample_rate: SampleRate, max_frames: usize) {
        // An adopting node has nothing to activate: the instance it takes
        // over was activated in the graph it comes from, at the same rate
        // and block size (the live graph refuses any other).
        if let Some(held) = self.held_mut() {
            if held.instance.is_active() {
                held.instance.deactivate();
            }
            // A failure leaves the instance inactive and the node silent.
            let _ = held.instance.activate(sample_rate, max_frames);
            let latency = held.instance.latency_frames();
            self.measured.store(latency, Ordering::Relaxed);
        }
    }

    fn reset(&mut self) {
        if let Some(held) = self.held_mut() {
            held.instance.reset();
        }
    }

    fn carry(&mut self) -> Option<(u64, &mut Option<Box<dyn Any + Send>>)> {
        self.key.map(|key| (key, &mut self.held))
    }

    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        input: &PlanarBuffer,
        output: &mut PlanarBuffer,
    ) -> ProcessStatus {
        let bypass = self.bypass.load(Ordering::Relaxed);
        let through = |output: &mut PlanarBuffer| {
            if ctx.input_is_silent || input.channels() != output.channels() {
                return ProcessStatus::Silent;
            }
            match output.copy_block_from(0, input, ctx.frames) {
                Ok(()) => ProcessStatus::Normal,
                Err(_) => ProcessStatus::Silent,
            }
        };
        // Split the borrow: the queue and the instance are separate fields.
        let Some(held) = self.held.as_mut().and_then(|h| h.downcast_mut::<Held>()) else {
            // Nothing has reached an adopting node yet: the signal passes.
            return through(output);
        };
        if !held.instance.is_active() {
            return ProcessStatus::Silent;
        }
        while let Some(change) = self.params_rx.pop() {
            // Unknown IDs / full plugin queues are dropped: nothing can be
            // reported from the audio thread.
            let _ = held
                .instance
                .set_param_at(change.id, change.value, change.frame);
        }
        if bypass {
            return through(output);
        }
        let status = held.instance.process(ctx, input, output);
        let Held { instance, changes } = held;
        // A full queue drops the change: nobody is reading.
        instance.drain_changes(&mut |change| {
            let _ = changes.push(change);
        });
        status
    }

    fn latency_frames(&self) -> usize {
        self.held()
            .map_or(self.latency, |h| h.instance.latency_frames())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::primitives::graph::nodes::{ConstantNode, PassThroughNode};
    use crate::primitives::graph::{AudioGraph, BlockTime};
    use crate::primitives::midi::MidiEventBuffer;
    use crate::primitives::plugins::{AudioPortLayout, ParamInfo, PluginDescriptor, PluginFormat};
    use crate::primitives::{Result, SuperbError};

    /// A pure-Rust gain plugin used to test the adapter without FFI.
    pub(crate) struct MockGain {
        pub desc: PluginDescriptor,
        pub params: Vec<ParamInfo>,
        pub gain: f64,
        pub active: bool,
        pub fail_activate: bool,
        pub latency: usize,
        pub resets: usize,
        pub queue: Vec<(u32, f64)>,
    }

    impl MockGain {
        pub fn new(latency: usize) -> Self {
            Self {
                desc: PluginDescriptor::new(PluginFormat::Clap, "superb.mock.gain", "Mock Gain"),
                params: vec![ParamInfo {
                    id: 1,
                    name: "Gain".into(),
                    module: String::new(),
                    min: 0.0,
                    max: 2.0,
                    default: 1.0,
                    automatable: true,
                    stepped: false,
                    hidden: false,
                    readonly: false,
                    bypass: false,
                }],
                gain: 1.0,
                active: false,
                fail_activate: false,
                latency,
                resets: 0,
                queue: Vec::with_capacity(8),
            }
        }
    }

    impl PluginInstance for MockGain {
        fn descriptor(&self) -> &PluginDescriptor {
            &self.desc
        }
        fn ports(&self) -> AudioPortLayout {
            AudioPortLayout {
                inputs: 2,
                outputs: 2,
            }
        }
        fn params(&self) -> &[ParamInfo] {
            &self.params
        }
        fn param_value(&self, id: u32) -> Result<f64> {
            if id == 1 {
                Ok(self.gain)
            } else {
                Err(SuperbError::NotFound {
                    kind: "param",
                    id: id as u64,
                })
            }
        }
        fn set_param(&mut self, id: u32, value: f64) -> Result<()> {
            if id != 1 {
                return Err(SuperbError::NotFound {
                    kind: "param",
                    id: id as u64,
                });
            }
            if self.active {
                if self.queue.len() == self.queue.capacity() {
                    return Err(SuperbError::CapacityExceeded {
                        what: "mock queue",
                        capacity: 8,
                    });
                }
                self.queue.push((id, value));
            } else {
                self.gain = value;
            }
            Ok(())
        }
        fn activate(&mut self, _sample_rate: SampleRate, _max_frames: usize) -> Result<()> {
            if self.fail_activate {
                return Err(SuperbError::Message("mock refuses to activate".into()));
            }
            self.active = true;
            Ok(())
        }
        fn deactivate(&mut self) {
            self.active = false;
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn reset(&mut self) {
            self.resets += 1;
        }
        fn process(
            &mut self,
            _ctx: &ProcessContext<'_>,
            input: &PlanarBuffer,
            output: &mut PlanarBuffer,
        ) -> ProcessStatus {
            for (_, v) in self.queue.drain(..) {
                self.gain = v;
            }
            output.copy_from(input).unwrap();
            output.apply_gain(self.gain as f32);
            ProcessStatus::Normal
        }
        fn latency_frames(&self) -> usize {
            self.latency
        }
        fn save_state(&mut self) -> Result<Vec<u8>> {
            Ok(self.gain.to_le_bytes().to_vec())
        }
        fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
            let arr: [u8; 8] = bytes
                .try_into()
                .map_err(|_| SuperbError::InvalidArgument { what: "state" })?;
            self.gain = f64::from_le_bytes(arr);
            Ok(())
        }
    }

    fn no_midi() -> MidiEventBuffer {
        MidiEventBuffer::with_capacity(0)
    }

    #[test]
    fn node_reports_plugin_layout_name_and_latency() {
        let mut node = PluginNode::new(Box::new(MockGain::new(12)));
        assert_eq!(node.name(), "Mock Gain");
        assert_eq!(node.channels(), NodeChannels::symmetric(2));
        assert_eq!(node.latency_frames(), 12);
        assert!(!node.is_prepared());
        assert!(node.take_param_sender().is_some());
        assert!(
            node.take_param_sender().is_none(),
            "sender is handed out once"
        );
        assert_eq!(node.instance().unwrap().descriptor().id, "superb.mock.gain");
        node.instance_mut().unwrap().set_param(1, 0.25).unwrap();
        assert_eq!(node.into_instance().unwrap().param_value(1).unwrap(), 0.25);
    }

    #[test]
    fn queued_params_reach_the_plugin_inside_the_graph() {
        let mut node = PluginNode::new(Box::new(MockGain::new(4)));
        let mut tx = node.take_param_sender().unwrap();
        let mut g = AudioGraph::new();
        let src = g.add_node(Box::new(ConstantNode::new(1.0, 2)));
        let fx = g.add_node(Box::new(node));
        let sink = g.add_node(Box::new(PassThroughNode::new(2)));
        g.connect(src, fx).unwrap();
        g.connect(fx, sink).unwrap();
        g.connect(src, sink).unwrap(); // parallel dry path, compensated by 4 frames
        g.set_output(sink).unwrap();
        g.prepare(SampleRate::HZ_48K, 8).unwrap();
        assert_eq!(g.latency_frames(), 4);
        assert_eq!(g.edge_delay(src, sink).unwrap(), 4);

        g.process(BlockTime::free_running(), 8, &no_midi(), None)
            .unwrap();
        assert_eq!(g.node_output(fx).unwrap().channel(1), &[1.0; 8]);

        tx.push(ParamChange::new(1, 0.5)).unwrap();
        tx.push(ParamChange::new(99, 7.0)).unwrap(); // unknown: dropped silently
        g.process(BlockTime::free_running(), 8, &no_midi(), None)
            .unwrap();
        assert_eq!(g.node_output(fx).unwrap().channel(0), &[0.5; 8]);
        g.reset();
        let plugin = g.remove_node(fx).unwrap();
        assert_eq!(plugin.name(), "Mock Gain");
    }

    #[test]
    fn bypass_passes_input_through_without_running_the_plugin() {
        let mut node = PluginNode::new(Box::new(MockGain::new(0)));
        let mut tx = node.take_param_sender().unwrap();
        let bypass = node.bypass_handle();
        node.prepare(SampleRate::HZ_48K, 8);
        let mut input = PlanarBuffer::new(2, 8);
        input.fill(1.0);
        let mut output = PlanarBuffer::new(2, 8);
        let midi = no_midi();
        let ctx = ProcessContext {
            sample_rate: SampleRate::HZ_48K,
            frames: 8,
            time: BlockTime::free_running(),
            midi: &midi,
            input_is_silent: false,
            input: None,
        };
        tx.push(ParamChange::new(1, 0.5)).unwrap();
        bypass.store(true, Ordering::Relaxed);
        assert!(node.is_bypassed());
        assert_eq!(
            node.process(&ctx, &input, &mut output),
            ProcessStatus::Normal
        );
        assert_eq!(
            output.channel(0),
            &[1.0; 8],
            "input copied, gain not applied"
        );
        // The queued change was handed to the plugin and takes effect when
        // it next runs.
        bypass.store(false, Ordering::Relaxed);
        assert_eq!(
            node.process(&ctx, &input, &mut output),
            ProcessStatus::Normal
        );
        assert_eq!(output.channel(0), &[0.5; 8]);
        // Silent input stays silent either way.
        let quiet = ProcessContext {
            input_is_silent: true,
            input: None,
            ..ctx
        };
        bypass.store(true, Ordering::Relaxed);
        assert_eq!(
            node.process(&quiet, &input, &mut output),
            ProcessStatus::Silent
        );
    }

    #[test]
    fn failed_activation_makes_the_node_silent() {
        let mut mock = MockGain::new(0);
        mock.fail_activate = true;
        let mut node = PluginNode::new(Box::new(mock));
        node.prepare(SampleRate::HZ_48K, 16);
        assert!(!node.is_prepared());
        let input = PlanarBuffer::new(2, 16);
        let mut output = PlanarBuffer::new(2, 16);
        let midi = no_midi();
        let ctx = ProcessContext {
            sample_rate: SampleRate::HZ_48K,
            frames: 16,
            time: BlockTime::free_running(),
            midi: &midi,
            input_is_silent: false,
            input: None,
        };
        assert_eq!(
            node.process(&ctx, &input, &mut output),
            ProcessStatus::Silent
        );

        // Re-preparing an active plugin deactivates first.
        let mut node = PluginNode::new(Box::new(MockGain::new(0)));
        node.prepare(SampleRate::HZ_48K, 16);
        node.prepare(SampleRate::CD, 32);
        assert!(node.is_prepared() && node.instance().unwrap().is_active());
        node.reset();
        assert_eq!(
            node.process(&ctx, &input, &mut output),
            ProcessStatus::Normal
        );
    }
}
