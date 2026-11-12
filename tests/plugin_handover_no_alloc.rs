//! Proves that a plugin instance moves from the outgoing graph to the
//! incoming one at a swap without a heap operation, without a block of dry
//! signal, and without being dropped on the audio thread, and that what
//! it reports changing itself keeps arriving on the one queue throughout.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::graph::nodes::ConstantNode;
use superb::primitives::graph::{live_pair, BlockTime, GraphEditor, ProcessContext, ProcessStatus};
use superb::primitives::midi::MidiEventBuffer;
use superb::primitives::plugins::{
    AudioPortLayout, ParamChange, ParamInfo, PluginChange, PluginDescriptor, PluginFormat,
    PluginInstance, PluginNode,
};
use superb::primitives::realtime::spsc::Producer;
use superb::primitives::realtime::thread::with_rt;
use superb::primitives::realtime::CountingAllocator;
use superb::primitives::{Result, SampleRate};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::system();

const R: SampleRate = SampleRate::HZ_48K;
const KEY: u64 = 7;
const PORTS: AudioPortLayout = AudioPortLayout {
    inputs: 2,
    outputs: 2,
};

/// A gain whose value is its only state, counting activations and drops.
/// It reports its gain as a change of its own after every block.
struct Gain {
    desc: PluginDescriptor,
    gain: f64,
    reported: bool,
    active: bool,
    activations: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Drop for Gain {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl PluginInstance for Gain {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.desc
    }
    fn ports(&self) -> AudioPortLayout {
        PORTS
    }
    fn params(&self) -> &[ParamInfo] {
        &[]
    }
    fn param_value(&self, _: u32) -> Result<f64> {
        Ok(self.gain)
    }
    fn set_param(&mut self, _: u32, value: f64) -> Result<()> {
        self.gain = value;
        Ok(())
    }
    fn activate(&mut self, _: SampleRate, _: usize) -> Result<()> {
        self.active = true;
        self.activations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.active = false;
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn reset(&mut self) {}
    fn process(
        &mut self,
        _: &ProcessContext<'_>,
        input: &PlanarBuffer,
        output: &mut PlanarBuffer,
    ) -> ProcessStatus {
        output.copy_from(input).unwrap();
        output.apply_gain(self.gain as f32);
        self.reported = false;
        ProcessStatus::Normal
    }
    fn drain_changes(&mut self, sink: &mut dyn FnMut(PluginChange)) {
        if !self.reported {
            self.reported = true;
            sink(PluginChange::Value {
                id: 0,
                value: self.gain,
            });
        }
    }
    fn latency_frames(&self) -> usize {
        0
    }
    fn save_state(&mut self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
    fn load_state(&mut self, _: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Publishes `constant → plugin node → output` and returns the node's
/// parameter queue.
fn publish(editor: &mut GraphEditor, mut node: PluginNode) -> Producer<ParamChange> {
    let params = node.take_param_sender().unwrap();
    let graph = editor.graph_mut();
    let source = graph.add_node(Box::new(ConstantNode::new(1.0, 2)));
    let plugin = graph.add_node(Box::new(node));
    graph.connect(source, plugin).unwrap();
    graph.set_output(plugin).unwrap();
    editor.publish().unwrap();
    params
}

#[test]
fn a_plugin_instance_crosses_a_graph_swap_without_allocating_or_dropping_out() {
    let activations = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (mut editor, mut live) = live_pair();
    editor.configure(R, 64);
    let midi = MidiEventBuffer::with_capacity(0);

    let instance = Gain {
        desc: PluginDescriptor::new(PluginFormat::Clap, "superb.test.gain", "Gain"),
        gain: 1.0,
        reported: true,
        active: false,
        activations: activations.clone(),
        drops: drops.clone(),
    };
    let mut node = PluginNode::new(Box::new(instance)).carried_as(KEY);
    let mut changes = node.take_change_receiver().unwrap();
    let mut params = publish(&mut editor, node);
    // The gain is state the instance holds and no rebuild could recreate.
    params.push(ParamChange::new(0, 0.25)).unwrap();
    live.process(BlockTime::free_running(), 64, &midi, None)
        .unwrap();
    assert_eq!(live.output().channel(0), &[0.25; 64]);
    assert_eq!(activations.load(Ordering::SeqCst), 1);

    // Three rebuilds in a row, each graph taking the instance from the last.
    let mut time = BlockTime::free_running();
    for round in 0..3 {
        let mut adopting = PluginNode::adopting(KEY, "Gain", PORTS, 0);
        assert!(
            adopting.take_change_receiver().is_none(),
            "the queue comes with the instance"
        );
        publish(&mut editor, adopting);
        with_rt(|| {
            superb::primitives::realtime::assert_no_alloc(|| {
                for _ in 0..4 {
                    live.process(time, 64, &midi, None).unwrap();
                    time = time.advanced_by(64, R);
                    // From the first block after the swap: no dry block.
                    assert!(live.output().channel(0).iter().all(|&s| s == 0.25));
                }
            });
        });
        assert_eq!(live.swap_count(), round + 2);
        // The retired graph is freed here, on the control thread, and the
        // instance is not in it any more.
        editor.try_reclaim();
        assert_eq!(drops.load(Ordering::SeqCst), 0, "round {round}");
    }
    assert_eq!(
        activations.load(Ordering::SeqCst),
        1,
        "never activated again"
    );
    // One report per block, from four graphs, on the queue taken once.
    let mut reports = 0;
    while let Some(change) = changes.pop() {
        assert_eq!(change, PluginChange::Value { id: 0, value: 0.25 });
        reports += 1;
    }
    assert_eq!(reports, 13);

    // A graph that no longer asks for the key leaves the instance in the
    // outgoing graph, which is freed on the control thread.
    let graph = editor.graph_mut();
    let source = graph.add_node(Box::new(ConstantNode::new(1.0, 2)));
    graph.set_output(source).unwrap();
    editor.publish().unwrap();
    with_rt(|| {
        superb::primitives::realtime::assert_no_alloc(|| {
            live.process(time, 64, &midi, None).unwrap();
        });
    });
    assert_eq!(drops.load(Ordering::SeqCst), 0, "not dropped by the swap");
    editor.try_reclaim();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn an_adopting_node_with_nothing_to_adopt_passes_its_input() {
    let (mut editor, mut live) = live_pair();
    editor.configure(R, 64);
    let midi = MidiEventBuffer::with_capacity(0);
    publish(&mut editor, PluginNode::adopting(KEY, "Gain", PORTS, 0));
    live.process(BlockTime::free_running(), 64, &midi, None)
        .unwrap();
    assert_eq!(live.output().channel(0), &[1.0; 64]);
}
