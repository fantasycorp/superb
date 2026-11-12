//! Format-agnostic plugin model.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::primitives::buffers::PlanarBuffer;
use crate::primitives::graph::{ProcessContext, ProcessStatus};
use crate::primitives::{Result, SampleRate};

/// A plugin API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    /// CLAP (`.clap` bundles).
    Clap,
    /// VST3 — stubbed, see [`crate::primitives::plugins::vst3`].
    Vst3,
}

impl PluginFormat {
    /// File extension of a bundle of this format (without the dot).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Clap => "clap",
            Self::Vst3 => "vst3",
        }
    }
}

/// What a plugin says about itself before it is instantiated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDescriptor {
    /// Format the plugin was found in.
    pub format: PluginFormat,
    /// Stable, unique ID (e.g. `com.vendor.plugin`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Vendor / author.
    pub vendor: String,
    /// Version string as reported by the plugin.
    pub version: String,
    /// Free-form description.
    pub description: String,
    /// Feature tags (`instrument`, `audio-effect`, `stereo`, ...).
    pub features: Vec<String>,
    /// Bundle the plugin was loaded from.
    pub path: PathBuf,
}

impl PluginDescriptor {
    /// Creates a minimal descriptor (tests, in-process plugins).
    ///
    /// ```
    /// use superb::primitives::plugins::{AudioPortLayout, PluginDescriptor, PluginFormat};
    ///
    /// let mut descriptor = PluginDescriptor::new(PluginFormat::Clap, "a.b", "AB");
    /// assert!(!descriptor.is_instrument());
    /// assert!(!descriptor.is_audio_effect());
    /// descriptor.features = vec!["instrument".into(), "synthesizer".into()];
    /// assert!(descriptor.is_instrument());
    /// descriptor.features.push("audio-effect".into());
    /// assert!(descriptor.is_audio_effect());
    /// assert_eq!(PluginFormat::Clap.extension(), "clap");
    /// assert_eq!(PluginFormat::Vst3.extension(), "vst3");
    /// assert_eq!(
    ///     AudioPortLayout::default(),
    ///     AudioPortLayout {
    ///         inputs: 0,
    ///         outputs: 0
    ///     }
    /// );
    /// ```
    pub fn new(format: PluginFormat, id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            format,
            id: id.into(),
            name: name.into(),
            vendor: String::new(),
            version: String::new(),
            description: String::new(),
            features: Vec::new(),
            path: PathBuf::new(),
        }
    }

    /// `true` if the plugin tags itself as an instrument.
    pub fn is_instrument(&self) -> bool {
        self.features.iter().any(|f| f == "instrument")
    }

    /// `true` if the plugin tags itself as an audio effect.
    pub fn is_audio_effect(&self) -> bool {
        self.features.iter().any(|f| f == "audio-effect")
    }
}

/// One automatable parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    /// Stable parameter ID (format-specific numbering).
    pub id: u32,
    /// Display name.
    pub name: String,
    /// Grouping path (`"Filter/Envelope"`), empty if none.
    pub module: String,
    /// Smallest value.
    pub min: f64,
    /// Largest value.
    pub max: f64,
    /// Value after instantiation.
    pub default: f64,
    /// May be automated by the host.
    pub automatable: bool,
    /// Takes integer steps only.
    pub stepped: bool,
    /// Not meant for the user interface.
    pub hidden: bool,
    /// Cannot be written by the host.
    pub readonly: bool,
    /// This is the plugin's bypass switch.
    pub bypass: bool,
}

/// Channel counts of a plugin's main audio input and output ports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioPortLayout {
    /// Channels on the main input (`0` for instruments / generators).
    pub inputs: usize,
    /// Channels on the main output (`0` for analysers).
    pub outputs: usize,
}

/// A parameter move queued for the audio thread.
///
/// ```
/// use superb::primitives::plugins::ParamChange;
///
/// let now = ParamChange::new(7, 0.5);
/// assert_eq!((now.id, now.value, now.frame), (7, 0.5, 0));
/// assert_eq!(now.at(32).frame, 32);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamChange {
    /// [`ParamInfo::id`].
    pub id: u32,
    /// New plain value in `[min, max]`.
    pub value: f64,
    /// Where in the block the change lands: frames after the start of the
    /// block that picks it up. `0` is the start of the block; a frame past
    /// the end of the block lands on its last frame. A format that cannot
    /// place a change inside a block applies it at the start.
    pub frame: u32,
}

impl ParamChange {
    /// A change that lands at the start of the next block.
    pub fn new(id: u32, value: f64) -> Self {
        Self {
            id,
            value,
            frame: 0,
        }
    }

    /// The same change, landing `frame` frames into the block (builder
    /// style).
    pub fn at(mut self, frame: u32) -> Self {
        self.frame = frame;
        self
    }
}

/// Something a plugin did to one of its own parameters: the user turned a
/// control in the plugin's own view, or the plugin moved it itself. The
/// host hears of it after the block it happened in
/// ([`PluginInstance::drain_changes`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PluginChange {
    /// The parameter now has this plain value.
    Value {
        /// [`ParamInfo::id`].
        id: u32,
        /// The value the plugin gave it.
        value: f64,
    },
    /// The user took hold of the parameter's control. Values follow until
    /// [`Released`](Self::Released).
    Touched {
        /// [`ParamInfo::id`].
        id: u32,
    },
    /// The user let go of the parameter's control.
    Released {
        /// [`ParamInfo::id`].
        id: u32,
    },
}

/// A hosted plugin, whatever its format.
///
/// Threading contract (mirrors CLAP's): `activate`, `deactivate`,
/// `param_value`, `save_state`, `load_state` and `params` are main-thread
/// calls; `process`, `reset`, `drain_changes`, `set_param` and
/// `set_param_at` (which only queue) are audio-thread safe once active and
/// must not allocate.
pub trait PluginInstance: Send {
    /// Static description.
    fn descriptor(&self) -> &PluginDescriptor;

    /// Main audio port channel counts.
    fn ports(&self) -> AudioPortLayout;

    /// All parameters, in the plugin's order.
    fn params(&self) -> &[ParamInfo];

    /// Current plain value of a parameter.
    fn param_value(&self, id: u32) -> Result<f64>;

    /// Queues a parameter change for the next `process` call (or applies it
    /// immediately while inactive). Fails for unknown IDs or a full queue.
    fn set_param(&mut self, id: u32, value: f64) -> Result<()>;

    /// As [`set_param`](Self::set_param), landing `frame` frames into the
    /// next block ([`ParamChange::frame`]). The default applies the change
    /// at the start of the block: a format that can place a change inside
    /// a block overrides it.
    fn set_param_at(&mut self, id: u32, value: f64, frame: u32) -> Result<()> {
        let _ = frame;
        self.set_param(id, value)
    }

    /// Hands `sink` every change the plugin made to its own parameters
    /// since the last call, in order ([`PluginChange`]). Audio-thread
    /// safe and must not allocate. The default has nothing to hand over.
    fn drain_changes(&mut self, sink: &mut dyn FnMut(PluginChange)) {
        let _ = sink;
    }

    /// Prepares for processing at `sample_rate` with blocks of at most
    /// `max_frames`. May allocate.
    fn activate(&mut self, sample_rate: SampleRate, max_frames: usize) -> Result<()>;

    /// Releases processing resources.
    fn deactivate(&mut self);

    /// `true` between a successful `activate` and `deactivate`.
    fn is_active(&self) -> bool;

    /// Clears internal state (voices, delay lines) without reallocating.
    fn reset(&mut self);

    /// Renders one block. `input`/`output` follow [`ports`](Self::ports).
    fn process(
        &mut self,
        ctx: &ProcessContext<'_>,
        input: &PlanarBuffer,
        output: &mut PlanarBuffer,
    ) -> ProcessStatus;

    /// Processing latency once active.
    fn latency_frames(&self) -> usize;

    /// Serialises the plugin's full state.
    fn save_state(&mut self) -> Result<Vec<u8>>;

    /// Restores state produced by [`save_state`](Self::save_state).
    fn load_state(&mut self, bytes: &[u8]) -> Result<()>;

    /// The main-thread side of this instance, for a caller that has handed
    /// the instance itself to the audio thread and still has to read its
    /// parameters or save its state. `None` when the format gives no way
    /// to do that beside `process`; the default.
    fn control(&self) -> Option<Box<dyn PluginControl>> {
        None
    }
}

/// The main-thread side of a plugin whose instance runs on the audio
/// thread: what a host may ask of a plugin while it processes.
///
/// A [`PluginInstance`] is owned by whoever processes it, so once it sits
/// in a running graph the control thread cannot reach it. This handle,
/// taken with [`PluginInstance::control`] before the instance is handed
/// over, is how the control thread still reads a parameter and saves or
/// restores the state. Every call fails cleanly once the instance has been
/// dropped.
pub trait PluginControl: Send {
    /// Current plain value of a parameter.
    fn param_value(&self, id: u32) -> Result<f64>;

    /// Serialises the plugin's full state.
    fn save_state(&self) -> Result<Vec<u8>>;

    /// Restores state produced by `save_state`.
    fn load_state(&self, bytes: &[u8]) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_helpers() {
        let mut d = PluginDescriptor::new(PluginFormat::Clap, "a.b", "AB");
        assert!(!d.is_instrument() && !d.is_audio_effect());
        d.features = vec!["instrument".into(), "synthesizer".into()];
        assert!(d.is_instrument());
        d.features.push("audio-effect".into());
        assert!(d.is_audio_effect());
        assert_eq!(PluginFormat::Clap.extension(), "clap");
        assert_eq!(PluginFormat::Vst3.extension(), "vst3");
        assert_eq!(
            AudioPortLayout::default(),
            AudioPortLayout {
                inputs: 0,
                outputs: 0
            }
        );
    }
}
