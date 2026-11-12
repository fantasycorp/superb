//! Inserts: the effects in a track's chain, as the memory remembers them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::primitives::plugins::{PluginDescriptor, PluginFormat};
use crate::primitives::InsertId;

/// Which plugin an insert refers to: enough to load it again from disk.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginRef {
    /// The plugin API the bundle speaks.
    pub format: PluginFormat,
    /// Bundle the plugin lives in.
    pub path: PathBuf,
    /// The plugin's stable ID inside that bundle (for example `com.vendor.plugin`).
    pub id: String,
}

impl From<&PluginDescriptor> for PluginRef {
    fn from(desc: &PluginDescriptor) -> Self {
        Self {
            format: desc.format,
            path: desc.path.clone(),
            id: desc.id.clone(),
        }
    }
}

/// What an insert is: one of the effects built into the crate, or a plugin
/// loaded from disk.
///
/// ```
/// use superb::primitives::session::{Effect, EffectParam};
///
/// assert_eq!(
///     Effect::Limiter.params(),
///     [EffectParam::Ceiling, EffectParam::Release]
/// );
/// assert!(Effect::Compressor.has(EffectParam::Ratio));
/// assert!(!Effect::Limiter.has(EffectParam::Ratio));
/// assert_eq!(EffectParam::from_id(EffectParam::Knee.id()), Some(EffectParam::Knee));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Turns down what is above a threshold (`graph::dynamics::Compressor`).
    Compressor,
    /// Keeps true peaks under a ceiling, with lookahead
    /// (`graph::dynamics::Limiter`).
    Limiter,
    /// Turns down what is below a threshold (`graph::dynamics::Gate`).
    Gate,
    /// Turns down sibilance in a band (`graph::dynamics::DeEsser`).
    DeEsser,
    /// Rides the level toward a target (`graph::dynamics::Leveler`).
    Leveler,
    /// A plugin loaded from disk. Stored, but not built into the mixer yet.
    Plugin(PluginRef),
}

impl Effect {
    /// The parameters this effect has, in the order a panel would show
    /// them. A plugin's parameters are its own and are not listed here.
    pub fn params(&self) -> &'static [EffectParam] {
        use EffectParam::*;
        match self {
            Effect::Compressor => &[Threshold, Ratio, Attack, Release, Knee, Makeup],
            Effect::Limiter => &[Ceiling, Release],
            Effect::Gate => &[Threshold, Ratio, Attack, Hold, Release, Range],
            Effect::DeEsser => &[Frequency, Threshold, Ratio],
            Effect::Leveler => &[Target, Range, Speed, Floor],
            Effect::Plugin(_) => &[],
        }
    }

    /// Whether this effect has `param`.
    pub fn has(&self, param: EffectParam) -> bool {
        self.params().contains(&param)
    }
}

impl From<PluginRef> for Effect {
    fn from(plugin: PluginRef) -> Self {
        Effect::Plugin(plugin)
    }
}

/// A parameter of a built-in [`Effect`]. Levels are in dB, times in
/// milliseconds, frequencies in Hz; each variant says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectParam {
    /// Where the effect begins to act, in dBFS.
    Threshold,
    /// How hard it acts past the threshold, as `n` in `n:1`.
    Ratio,
    /// How fast it reacts, in milliseconds.
    Attack,
    /// How fast it lets go, in milliseconds.
    Release,
    /// The width of the soft knee around the threshold, in dB.
    Knee,
    /// Gain added after compression, in dB.
    Makeup,
    /// A limiter's true-peak ceiling, in dBTP.
    Ceiling,
    /// How long a gate stays open after the signal drops, in milliseconds.
    Hold,
    /// The most the effect may turn the signal down (a gate) or move it
    /// either way (a leveler), in dB.
    Range,
    /// The centre of the band a de-esser listens to, in Hz.
    Frequency,
    /// The level a leveler rides toward, in dBFS.
    Target,
    /// How fast a leveler moves, in dB per second.
    Speed,
    /// Below this level, in dBFS, a leveler holds its gain.
    Floor,
}

impl EffectParam {
    const ALL: [EffectParam; 13] = [
        EffectParam::Threshold,
        EffectParam::Ratio,
        EffectParam::Attack,
        EffectParam::Release,
        EffectParam::Knee,
        EffectParam::Makeup,
        EffectParam::Ceiling,
        EffectParam::Hold,
        EffectParam::Range,
        EffectParam::Frequency,
        EffectParam::Target,
        EffectParam::Speed,
        EffectParam::Floor,
    ];

    /// The id this parameter is stored under in [`InsertState::params`].
    /// Fixed: session files hold it.
    pub fn id(self) -> u32 {
        match self {
            EffectParam::Threshold => 1,
            EffectParam::Ratio => 2,
            EffectParam::Attack => 3,
            EffectParam::Release => 4,
            EffectParam::Knee => 5,
            EffectParam::Makeup => 6,
            EffectParam::Ceiling => 7,
            EffectParam::Hold => 8,
            EffectParam::Range => 9,
            EffectParam::Frequency => 10,
            EffectParam::Target => 11,
            EffectParam::Speed => 12,
            EffectParam::Floor => 13,
        }
    }

    /// The parameter stored under `id`, if it is one of these.
    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
}

/// One effect in a track's insert chain.
///
/// ```
/// use superb::primitives::plugins::{PluginDescriptor, PluginFormat};
/// use superb::primitives::session::{Effect, EffectParam, Insert, PluginRef};
/// use superb::primitives::InsertId;
///
/// let mut comp = Insert::new(Effect::Compressor);
/// comp.set_param(EffectParam::Threshold.id(), -18.0);
/// assert_eq!(comp.param(EffectParam::Threshold.id()), Some(-18.0));
///
/// let mut descriptor = PluginDescriptor::new(PluginFormat::Clap, "com.example.verb", "Verb");
/// descriptor.path = "/plugins/verb.clap".into();
/// let plugin = PluginRef::from(&descriptor);
/// assert_eq!(plugin.id, "com.example.verb");
/// let mut insert = Insert::new(plugin.clone());
/// assert_eq!(insert.effect, Effect::Plugin(plugin.clone()));
/// assert_eq!(insert.id(), InsertId::INVALID);
/// assert_eq!(insert.param(7), None);
/// insert.set_param(7, 0.5);
/// insert.set_param(7, 0.25);
/// insert.set_param(9, 1.0);
/// assert_eq!(insert.param(7), Some(0.25));
/// assert_eq!(insert.state.params.len(), 2);
/// assert!(insert.clear_param(7));
/// assert!(!insert.clear_param(7));
/// assert_eq!(insert.param(9), Some(1.0));
/// let placed = Insert::with_id(InsertId::from_raw(3), plugin);
/// assert_eq!(placed.id(), InsertId::from_raw(3));
/// assert!(!placed.state.bypass);
/// assert!(placed.preset.is_empty());
/// ```
///
/// The memory stores what is needed to recreate the effect: which one,
/// its state (bypass and the parameter values the user has set), and a
/// plugin's own saved preset.
/// [`session::steps::build_mixer`](crate::session::steps::build_mixer)
/// turns every built-in effect that is not bypassed into a node on its
/// track's strip, in chain order. A [`Effect::Plugin`] is stored and
/// skipped: load it and place a
/// [`PluginNode`](crate::primitives::plugins::PluginNode) with
/// [`Mixer::add_insert`](crate::primitives::mixer::Mixer::add_insert) on a
/// mixer you built.
#[derive(Clone, Debug, PartialEq)]
pub struct Insert {
    pub(crate) id: InsertId,
    /// Which effect this is.
    pub effect: Effect,
    /// Bypass and parameter values.
    pub state: InsertState,
    /// Opaque bytes the plugin saved for itself, or empty. Restored before
    /// the parameter values are applied.
    pub preset: Vec<u8>,
}

/// An insert's state: whether it is bypassed and the parameter values the
/// user has set.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct InsertState {
    /// A bypassed insert passes its input through unchanged: the mixer is
    /// built without it.
    pub bypass: bool,
    /// Parameter values to apply after loading, as `(parameter id, plain
    /// value)`; a built-in effect's ids are [`EffectParam::id`]. Parameters
    /// not listed keep the effect's defaults.
    pub params: Vec<(u32, f64)>,
}

impl Insert {
    /// An insert of `effect` (a built-in [`Effect`] or a [`PluginRef`])
    /// with no ID yet (the memory assigns one).
    pub fn new(effect: impl Into<Effect>) -> Self {
        Self::with_id(InsertId::INVALID, effect)
    }

    /// An insert with a pre-allocated ID (see
    /// [`Memory::allocate_insert_id`](crate::primitives::session::Memory::allocate_insert_id)).
    pub fn with_id(id: InsertId, effect: impl Into<Effect>) -> Self {
        Self {
            id,
            effect: effect.into(),
            state: InsertState::default(),
            preset: Vec::new(),
        }
    }

    /// The insert's ID (`InsertId::INVALID` until added to a memory).
    pub fn id(&self) -> InsertId {
        self.id
    }

    /// The stored value of a parameter, if the user has set one.
    pub fn param(&self, param: u32) -> Option<f64> {
        self.state
            .params
            .iter()
            .find(|(p, _)| *p == param)
            .map(|(_, v)| *v)
    }

    /// Stores a parameter value, replacing any earlier value for it.
    pub fn set_param(&mut self, param: u32, value: f64) {
        match self.state.params.iter_mut().find(|(p, _)| *p == param) {
            Some(entry) => entry.1 = value,
            None => self.state.params.push((param, value)),
        }
    }

    /// Forgets a stored parameter value so the plugin's default applies.
    /// Returns whether one was stored.
    pub fn clear_param(&mut self, param: u32) -> bool {
        let before = self.state.params.len();
        self.state.params.retain(|(p, _)| *p != param);
        self.state.params.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> PluginRef {
        PluginRef {
            format: PluginFormat::Clap,
            path: "/plugins/verb.clap".into(),
            id: "com.example.verb".into(),
        }
    }

    #[test]
    fn params_are_upserted_and_cleared() {
        let mut insert = Insert::new(plugin());
        assert_eq!(insert.id(), InsertId::INVALID);
        assert_eq!(insert.param(7), None);
        insert.set_param(7, 0.5);
        insert.set_param(9, 1.0);
        insert.set_param(7, 0.25);
        assert_eq!(insert.param(7), Some(0.25));
        assert_eq!(insert.state.params.len(), 2);
        assert!(insert.clear_param(7));
        assert!(!insert.clear_param(7));
        assert_eq!(insert.param(9), Some(1.0));
    }

    #[test]
    fn every_effect_param_has_its_own_id_and_reads_back() {
        for (i, param) in EffectParam::ALL.into_iter().enumerate() {
            assert_eq!(EffectParam::from_id(param.id()), Some(param));
            assert!(EffectParam::ALL[..i].iter().all(|p| p.id() != param.id()));
        }
        assert_eq!(EffectParam::from_id(0), None);
        assert!(Effect::Plugin(plugin()).params().is_empty());
        assert!(Effect::Gate.has(EffectParam::Hold));
        assert!(!Effect::Compressor.has(EffectParam::Hold));
        let json = serde_json::to_string(&Effect::DeEsser).unwrap();
        assert_eq!(json, "\"de_esser\"");
        assert_eq!(
            serde_json::from_str::<Effect>(&json).unwrap(),
            Effect::DeEsser
        );
    }

    #[test]
    fn plugin_ref_comes_from_a_descriptor() {
        let mut desc = PluginDescriptor::new(PluginFormat::Clap, "com.example.verb", "Verb");
        desc.path = "/plugins/verb.clap".into();
        assert_eq!(PluginRef::from(&desc), plugin());
        let insert = Insert::with_id(InsertId::from_raw(3), plugin());
        assert_eq!(insert.id(), InsertId::from_raw(3));
        assert!(!insert.state.bypass && insert.preset.is_empty());
    }
}
