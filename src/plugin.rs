//! Plugins: find them, and work the ones on your tracks.
//!
//! A plugin goes on a track as an insert
//! ([`insert::add`](crate::insert::add) with `Effect::Plugin`), and
//! `insert::remove`, `move_to` and `bypass` work on it as on any insert.
//! This flow is the rest: [`scan`] for what is installed, a plugin's
//! [`params`], [`param`] to set one, and [`changes`] for what the plugin
//! changed itself, so a control drawn for it follows a knob turned in the
//! plugin's own view. A plugin is loaded once and kept running across
//! every rebuild of the mixer, with its state and its tails;
//! `session::save` stores its state in the session folder
//! (`plugins/<insert id>.state`).
//!
//! CLAP is the format hosted today. A plugin with no audio input (an
//! instrument) is refused for now.
//!
//! Made of `superb::primitives::plugins` (the host and `PluginNode`),
//! [`session::plugins`](crate::session::plugins) (what the session keeps
//! of a loaded plugin) and `superb::primitives::session::commands`.
//!
//! ```
//! use superb::primitives::session::{Effect, PluginRef, TrackKind};
//! use superb::primitives::SampleRate;
//! use superb::{insert, plugin, session, track};
//!
//! let folder = std::env::temp_dir().join(format!("superb-plugin-example-{}", std::process::id()));
//! let mut session = session::create(&folder, SampleRate::HZ_48K, 64)?;
//! let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
//! // Whatever CLAP plugins this machine has; none is fine.
//! for found in plugin::scan() {
//!     let placed = insert::add(&mut session, vox, Effect::Plugin(PluginRef::from(&found)));
//!     if let Ok(id) = placed {
//!         for info in plugin::params(&mut session, id)? {
//!             println!("{}: {} ({}..{})", found.name, info.name, info.min, info.max);
//!         }
//!         insert::remove(&mut session, id)?;
//!     }
//!     break;
//! }
//! let _ = std::fs::remove_dir_all(&folder);
//! # Ok::<(), superb::primitives::SuperbError>(())
//! ```

use crate::primitives::plugins::{scan as host_scan, ParamChange, ParamInfo};
use crate::primitives::plugins::{PluginChange, PluginDescriptor, PluginFormat};
use crate::primitives::session::commands;
use crate::primitives::{InsertId, Result, SuperbError};
use crate::session::plugins::{self, PluginSlot};
use crate::session::Session;

/// Every plugin installed where plugins are conventionally kept on this
/// machine, plus the folders `CLAP_PATH` names. A bundle that does not
/// load is skipped. Turn a result into an insert with
/// `Effect::Plugin(PluginRef::from(&descriptor))`.
///
/// Made of: `plugins::scan::scan_default(PluginFormat::Clap)`.
pub fn scan() -> Vec<PluginDescriptor> {
    host_scan::scan_default(PluginFormat::Clap)
        .map(|report| report.plugins)
        .unwrap_or_default()
}

/// Loads the plugin of an insert if the session has not yet (a session
/// just opened loads its plugins at the first rebuild).
fn ensure(session: &mut Session, insert: InsertId) -> Result<()> {
    let stored = session.memory.insert(insert)?.clone();
    plugins::ensure(
        &mut session.plugins,
        &mut session.plugin_loader,
        &stored,
        session.channels,
    );
    Ok(())
}

/// A plugin's parameters, in the plugin's order: each one's id, name,
/// range and default. Loads the plugin if the session has not yet.
///
/// Made of: [`Session::plugin_loader`] when needed → `LoadedPlugin::params`.
pub fn params(session: &mut Session, insert: InsertId) -> Result<Vec<ParamInfo>> {
    ensure(session, insert)?;
    Ok(plugins::loaded(session, insert)?.params.clone())
}

/// Sets one parameter of a plugin, by the id [`params`] lists, to a plain
/// value inside its range. Heard at the plugin's next block. Moves within
/// [`Session::COALESCE_WINDOW`] of each other are one undo step.
///
/// Made of: `set_insert_param` through `session::edit` →
/// `PluginHandle::params` (the running plugin's parameter queue).
pub fn param(session: &mut Session, insert: InsertId, param: u32, value: f64) -> Result<()> {
    ensure(session, insert)?;
    let info = plugins::loaded(session, insert)?
        .params
        .iter()
        .find(|info| info.id == param)
        .ok_or(SuperbError::NotFound {
            kind: "plugin parameter",
            id: u64::from(param),
        })?;
    if !value.is_finite() || value < info.min || value > info.max {
        return Err(SuperbError::InvalidArgument { what: "value" });
    }
    crate::session::edit(session, commands::set_insert_param(insert, param, value))?;
    if let Some(live) = session.mixer.plugin_mut(insert) {
        if live.params.push(ParamChange::new(param, value)).is_ok() {
            plugins::note_sent(session, insert, param, value);
        }
    }
    Ok(())
}

/// The value a parameter has right now, read from the plugin: what
/// [`param`] set, or what the plugin moved it to itself.
///
/// Made of: [`Session::plugin_loader`] when needed → `LoadedPlugin::value`
/// (`PluginControl::param_value` while the plugin runs).
pub fn value(session: &mut Session, insert: InsertId, param: u32) -> Result<f64> {
    ensure(session, insert)?;
    plugins::loaded(session, insert)?.value(param)
}

/// What the plugin changed itself since this was last asked, in order: a
/// control taken hold of in the plugin's own view
/// (`PluginChange::Touched`), each value it was moved to (`Value`), and
/// the control let go (`Released`). A plugin may also move a parameter
/// with no gesture around it. Ask regularly while a control for the
/// plugin is on screen, and draw what comes back; each call hands the
/// changes out once.
///
/// A value the plugin reports goes into the memory as [`param`] would put
/// it, without being sent back to the plugin: it is saved with the
/// session, and undo returns the parameter to what it was before.
/// `session::save` takes the values in too, so nothing is lost if this is
/// never called. A change is heard after the block it happens in, so
/// none arrive while no audio is running.
///
/// Made of: `PluginNode::take_change_receiver` (the queue the running
/// plugin reports on) → `set_insert_param` through `session::edit` per
/// parameter moved.
pub fn changes(session: &mut Session, insert: InsertId) -> Result<Vec<PluginChange>> {
    ensure(session, insert)?;
    plugins::hear(session);
    plugins::take_heard(session, insert)
}

/// The plugin inserts of the session whose plugin could not be loaded,
/// each with the reason. They stay in the session, are saved as they
/// were, and are left out of the mixer.
///
/// Made of: the `PluginSlot::Missing` entries of `Session::plugins`.
pub fn missing(session: &Session) -> Vec<(InsertId, SuperbError)> {
    session
        .plugins
        .iter()
        .filter_map(|(id, slot)| match slot {
            PluginSlot::Missing(error) => Some((*id, error.clone())),
            PluginSlot::Loaded(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::buffers::PlanarBuffer;
    use crate::primitives::graph::{ProcessContext, ProcessStatus};
    use crate::primitives::plugins::{AudioPortLayout, PluginControl, PluginInstance};
    use crate::primitives::session::{Effect, PluginRef, TrackKind};
    use crate::primitives::{SampleRate, TimePosition, TrackId};
    use crate::{export, insert, session, track, transport};
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const R: SampleRate = SampleRate::HZ_48K;
    const GAIN: u32 = 1;

    /// What the tests watch a loader do.
    #[derive(Default)]
    struct Counters {
        loads: AtomicUsize,
        live: AtomicUsize,
        activations: AtomicUsize,
    }

    /// A gain plugin. Its gain is its whole state, shared with its control
    /// handle the way a real plugin's is behind its two threads.
    struct Gain {
        desc: PluginDescriptor,
        params: Vec<ParamInfo>,
        gain: Arc<Mutex<f64>>,
        active: bool,
        counters: Arc<Counters>,
    }

    struct GainControl(Arc<Mutex<f64>>);

    impl PluginControl for GainControl {
        fn param_value(&self, _: u32) -> Result<f64> {
            Ok(*self.0.lock().unwrap())
        }
        fn save_state(&self) -> Result<Vec<u8>> {
            Ok(self.0.lock().unwrap().to_le_bytes().to_vec())
        }
        fn load_state(&self, bytes: &[u8]) -> Result<()> {
            let bytes: [u8; 8] = bytes
                .try_into()
                .map_err(|_| SuperbError::format(None, "not a gain"))?;
            *self.0.lock().unwrap() = f64::from_le_bytes(bytes);
            Ok(())
        }
    }

    impl Drop for Gain {
        fn drop(&mut self) {
            self.counters.live.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl PluginInstance for Gain {
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
            GainControl(self.gain.clone()).param_value(id)
        }
        fn set_param(&mut self, _: u32, value: f64) -> Result<()> {
            *self.gain.lock().unwrap() = value;
            Ok(())
        }
        fn activate(&mut self, _: SampleRate, _: usize) -> Result<()> {
            self.active = true;
            self.counters.activations.fetch_add(1, Ordering::SeqCst);
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
            output.apply_gain(*self.gain.lock().unwrap() as f32);
            ProcessStatus::Normal
        }
        fn latency_frames(&self) -> usize {
            0
        }
        fn save_state(&mut self) -> Result<Vec<u8>> {
            GainControl(self.gain.clone()).save_state()
        }
        fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
            GainControl(self.gain.clone()).load_state(bytes)
        }
        fn control(&self) -> Option<Box<dyn PluginControl>> {
            Some(Box::new(GainControl(self.gain.clone())))
        }
    }

    fn gain_ref() -> PluginRef {
        PluginRef {
            format: PluginFormat::Clap,
            path: "/plugins/gain.clap".into(),
            id: "superb.test.gain".into(),
        }
    }

    /// Makes the session load [`Gain`] for `gain_ref()` and fail for
    /// anything else.
    fn use_test_loader(session: &mut Session) -> Arc<Counters> {
        let counters = Arc::new(Counters::default());
        let seen = counters.clone();
        session.plugin_loader = Box::new(move |plugin: &PluginRef| {
            if plugin.id != "superb.test.gain" {
                return Err(SuperbError::io(
                    "open",
                    &plugin.path,
                    &std::io::Error::from(std::io::ErrorKind::NotFound),
                ));
            }
            seen.loads.fetch_add(1, Ordering::SeqCst);
            seen.live.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Gain {
                desc: PluginDescriptor::new(PluginFormat::Clap, "superb.test.gain", "Gain"),
                params: vec![ParamInfo {
                    id: GAIN,
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
                gain: Arc::new(Mutex::new(1.0)),
                active: false,
                counters: seen.clone(),
            }) as Box<dyn PluginInstance>)
        });
        counters
    }

    /// One audio track playing a constant 0.5 for a second, and the test
    /// loader in place.
    fn rig(tag: &str) -> (Session, TrackId, Arc<Counters>) {
        let mut session = session::create(session::scratch(tag), R, 64).unwrap();
        let counters = use_test_loader(&mut session);
        let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
        let mut tone = PlanarBuffer::new(2, 48_000);
        tone.fill(0.5);
        session.audio.insert("tone.wav", tone);
        crate::clip::add(&mut session, vox, "tone.wav", TimePosition::ZERO).unwrap();
        (session, vox, counters)
    }

    /// The level a few blocks in, played from the top.
    fn level(session: &mut Session) -> f32 {
        transport::rewind(session).unwrap();
        transport::start(session).unwrap();
        let audio = session::steps::render_blocks(session, 20).unwrap();
        audio.channel(0)[640..]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_plugin_insert_is_heard_and_follows_its_parameters_and_undo() {
        let (mut session, vox, counters) = rig("plugin-heard");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        assert_eq!(counters.loads.load(Ordering::SeqCst), 1);
        let infos = params(&mut session, gain).unwrap();
        assert_eq!((infos.len(), infos[0].name.as_str()), (1, "Gain"));
        assert!(close(level(&mut session), dry), "unity by default");

        // A parameter reaches the running plugin with no rebuild.
        param(&mut session, gain, GAIN, 0.5).unwrap();
        param(&mut session, gain, GAIN, 0.25).unwrap();
        assert!(!session.needs_rebuild);
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(value(&mut session, gain, GAIN).unwrap(), 0.25);
        assert_eq!(session.memory.insert(gain).unwrap().param(GAIN), Some(0.25));

        // Out of range, unknown, or not a plugin: refused.
        assert!(param(&mut session, gain, GAIN, 3.0).is_err());
        assert!(param(&mut session, gain, 99, 0.5).is_err());
        assert!(params(&mut session, InsertId::from_raw(999)).is_err());

        // Undo takes the drag back, and the plugin with it.
        assert!(session::undo(&mut session).unwrap());
        assert_eq!(session.memory.insert(gain).unwrap().param(GAIN), None);
        assert!(close(level(&mut session), dry));
        assert!(session::redo(&mut session).unwrap());
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(counters.loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_instance_is_loaded_once_and_outlives_every_rebuild() {
        let (mut session, vox, counters) = rig("plugin-outlives");
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.5).unwrap();
        let dry = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(level(&mut session), dry * 0.5));
        for name in ["a", "b", "c", "d"] {
            // Each of these replaces the whole mixer.
            track::add(&mut session, name, TrackKind::Audio).unwrap();
            assert!(session.needs_rebuild);
            assert!(close(level(&mut session), dry * 0.5), "{name}");
        }
        let comp = insert::add(&mut session, vox, Effect::Gate).unwrap();
        insert::move_to(&mut session, comp, 0).unwrap();
        assert!(close(level(&mut session), dry * 0.5));
        assert_eq!(counters.loads.load(Ordering::SeqCst), 1, "loaded once");
        assert_eq!(counters.live.load(Ordering::SeqCst), 1);
        assert_eq!(
            counters.activations.load(Ordering::SeqCst),
            1,
            "never activated again"
        );
    }

    #[test]
    fn bypass_switches_the_running_plugin_and_keeps_it_loaded() {
        let (mut session, vox, counters) = rig("plugin-bypass");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.25).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        let steps = session.history.undo_len();
        insert::bypass(&mut session, gain, true).unwrap();
        assert!(!session.needs_rebuild, "no rebuild for a plugin");
        assert_eq!(session.history.undo_len(), steps);
        assert!(close(level(&mut session), dry));
        // A rebuild keeps it bypassed, and loaded.
        session::steps::rebuild(&mut session).unwrap();
        assert!(close(level(&mut session), dry));
        insert::bypass(&mut session, gain, false).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(counters.loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_plugins_state_is_saved_with_the_session_and_loaded_on_open() {
        let (mut session, vox, _) = rig("plugin-saved");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.25).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        session::save(&mut session).unwrap();
        assert_eq!(
            session.memory.insert(gain).unwrap().preset,
            0.25f64.to_le_bytes()
        );
        // The bytes are in a file of their own, not in memory.json.
        let layout = session::layout(&session);
        assert_eq!(
            std::fs::read(layout.plugin_state(gain)).unwrap(),
            0.25f64.to_le_bytes()
        );
        let json = std::fs::read_to_string(layout.memory()).unwrap();
        assert!(!json.contains("preset"), "{json}");
        let dir = session.folder.clone();

        let mut again = session::open(&dir, 64).unwrap();
        let counters = use_test_loader(&mut again);
        assert!(again.plugins.is_empty(), "loaded at the first rebuild");
        assert!(close(level(&mut again), dry * 0.25));
        assert_eq!(counters.loads.load(Ordering::SeqCst), 1);
        assert_eq!(value(&mut again, gain, GAIN).unwrap(), 0.25);
        assert!(missing(&again).is_empty());

        // An insert that is gone takes its file with it at the next save,
        // and one that undo brings back gets it again.
        insert::remove(&mut again, gain).unwrap();
        session::save(&mut again).unwrap();
        assert!(!layout.plugin_state(gain).exists());
        assert!(session::undo(&mut again).unwrap());
        session::save(&mut again).unwrap();
        assert!(layout.plugin_state(gain).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plugin_state_inside_an_older_memory_file_still_loads() {
        let (mut session, vox, _) = rig("plugin-inline");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.5).unwrap();
        assert!(close(level(&mut session), dry * 0.5));
        session::save(&mut session).unwrap();
        // As written before plugin state had files of its own: the bytes
        // in memory.json, and no file.
        let layout = session::layout(&session);
        session
            .memory
            .insert_mut(gain)
            .unwrap()
            .state
            .params
            .clear();
        session.memory.save(layout.memory()).unwrap();
        std::fs::remove_file(layout.plugin_state(gain)).unwrap();
        assert!(std::fs::read_to_string(layout.memory())
            .unwrap()
            .contains("preset"));
        assert!(!layout.plugin_state(gain).exists());
        let dir = session.folder.clone();
        drop(session);

        let mut again = session::open(&dir, 64).unwrap();
        use_test_loader(&mut again);
        assert!(close(level(&mut again), dry * 0.5));
        // The next save moves it out.
        session::save(&mut again).unwrap();
        assert!(layout.plugin_state(gain).exists());
        assert!(!std::fs::read_to_string(layout.memory())
            .unwrap()
            .contains("preset"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_plugin_is_let_go_and_comes_back_as_it_was() {
        let (mut session, vox, counters) = rig("plugin-removed");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.25).unwrap();
        assert!(close(level(&mut session), dry * 0.25));

        insert::remove(&mut session, gain).unwrap();
        assert!(close(level(&mut session), dry));
        assert!(session.plugins.is_empty());
        assert_eq!(
            counters.live.load(Ordering::SeqCst),
            0,
            "freed with the mixer it was in, on this thread"
        );

        assert!(session::undo(&mut session).unwrap());
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(counters.loads.load(Ordering::SeqCst), 2, "loaded again");
        assert_eq!(counters.live.load(Ordering::SeqCst), 1);

        // Archiving the track lets the plugin go too, and restoring it
        // brings it back.
        track::archive(&mut session, vox).unwrap();
        assert_eq!(level(&mut session), 0.0);
        assert_eq!(counters.live.load(Ordering::SeqCst), 0);
        track::restore(&mut session, vox).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(counters.live.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_plugin_that_is_missing_leaves_the_session_playable() {
        let (mut session, vox, counters) = rig("plugin-missing");
        let dry = level(&mut session);
        let lost = PluginRef {
            format: PluginFormat::Clap,
            path: "/plugins/lost.clap".into(),
            id: "com.example.lost".into(),
        };
        assert!(matches!(
            insert::add(&mut session, vox, Effect::Plugin(lost.clone())),
            Err(SuperbError::Io { .. })
        ));
        assert_eq!(session.memory.insert_count(), 0);

        // A session saved on a machine that had the plugin.
        let id = session
            .memory
            .add_insert(vox, crate::primitives::session::Insert::new(lost))
            .unwrap();
        session.memory.insert_mut(id).unwrap().preset = vec![1, 2, 3];
        session.needs_rebuild = true;
        assert!(close(level(&mut session), dry), "plays without it");
        let gone = missing(&session);
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].0, id);
        assert!(params(&mut session, id).is_err());
        assert!(param(&mut session, id, GAIN, 0.5).is_err());
        // Saved as it was, and not asked for again at every rebuild.
        session::save(&mut session).unwrap();
        assert_eq!(session.memory.insert(id).unwrap().preset, [1, 2, 3]);
        assert_eq!(
            std::fs::read(session::layout(&session).plugin_state(id)).unwrap(),
            [1, 2, 3]
        );
        session::steps::rebuild(&mut session).unwrap();
        assert_eq!(counters.loads.load(Ordering::SeqCst), 0);
        let again = session::open(&session.folder, 64).unwrap();
        assert_eq!(again.memory.insert(id).unwrap().preset, [1, 2, 3]);
        let _ = std::fs::remove_dir_all(&session.folder);
    }

    /// The same path with a real CLAP plugin behind it: the host's own
    /// test plugin, linked into the test binary.
    #[cfg(feature = "clap")]
    #[test]
    fn a_clap_plugin_runs_in_the_session_and_is_read_and_saved_while_it_does() {
        use crate::primitives::plugins::clap::test_plugin::{
            self, serial, TEST_ENTRY, TEST_PLUGIN_ID,
        };
        use crate::primitives::plugins::clap::ClapBundle;
        let _guard = serial();
        let (mut session, vox, _) = rig("plugin-clap");
        session.plugin_loader = Box::new(|plugin: &PluginRef| {
            let bundle = ClapBundle::from_entry(&TEST_ENTRY, &plugin.path)?;
            Ok(Box::new(bundle.instantiate(&plugin.id)?) as Box<dyn PluginInstance>)
        });
        let dry = level(&mut session);
        let reference = PluginRef {
            format: PluginFormat::Clap,
            path: "/virtual/test.clap".into(),
            id: TEST_PLUGIN_ID.into(),
        };
        let gain = insert::add(&mut session, vox, Effect::Plugin(reference)).unwrap();
        assert!(params(&mut session, gain)
            .unwrap()
            .iter()
            .any(|p| p.id == 7));
        param(&mut session, gain, 7, 0.5).unwrap();
        assert!(close(level(&mut session), dry * 0.5));
        // Read and saved through the main-thread handle, while the
        // instance sits in the mixer.
        assert_eq!(value(&mut session, gain, 7).unwrap(), 0.5);
        session::save(&mut session).unwrap();
        assert_eq!(session.memory.insert(gain).unwrap().preset.len(), 8);
        // Rebuilds keep the one instance.
        track::add(&mut session, "other", TrackKind::Audio).unwrap();
        assert!(close(level(&mut session), dry * 0.5));
        assert_eq!(test_plugin::live_instances(), 1);
        // Opened again, it comes back from its saved state.
        let dir = session.folder.clone();
        drop(session);
        assert_eq!(test_plugin::live_instances(), 0);
        let mut again = session::open(&dir, 64).unwrap();
        again.plugin_loader = Box::new(|plugin: &PluginRef| {
            let bundle = ClapBundle::from_entry(&TEST_ENTRY, &plugin.path)?;
            Ok(Box::new(bundle.instantiate(&plugin.id)?) as Box<dyn PluginInstance>)
        });
        assert!(close(level(&mut again), dry * 0.5));
        drop(again);
        assert_eq!(test_plugin::live_instances(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "clap")]
    #[test]
    fn what_a_plugin_changes_itself_is_heard_kept_and_undone() {
        use crate::primitives::plugins::clap::test_plugin::{
            self, serial, TEST_ENTRY, TEST_PLUGIN_ID,
        };
        use crate::primitives::plugins::clap::ClapBundle;
        use crate::primitives::plugins::PluginChange;
        let _guard = serial();
        let (mut session, vox, _) = rig("plugin-changes");
        session.plugin_loader = Box::new(|plugin: &PluginRef| {
            let bundle = ClapBundle::from_entry(&TEST_ENTRY, &plugin.path)?;
            Ok(Box::new(bundle.instantiate(&plugin.id)?) as Box<dyn PluginInstance>)
        });
        let dry = level(&mut session);
        let reference = PluginRef {
            format: PluginFormat::Clap,
            path: "/virtual/test.clap".into(),
            id: TEST_PLUGIN_ID.into(),
        };
        let gain = insert::add(&mut session, vox, Effect::Plugin(reference)).unwrap();
        assert!(close(level(&mut session), dry));
        assert!(changes(&mut session, gain).unwrap().is_empty());

        // The user turns the gain in the plugin's own view.
        test_plugin::turn_gain_inside(0.25);
        assert!(close(level(&mut session), dry * 0.25));
        // Across a rebuild the report is still there to be heard.
        track::add(&mut session, "other", TrackKind::Audio).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        assert_eq!(
            changes(&mut session, gain).unwrap(),
            [
                PluginChange::Touched { id: 7 },
                PluginChange::Value { id: 7, value: 0.25 },
                PluginChange::Released { id: 7 },
            ]
        );
        assert!(changes(&mut session, gain).unwrap().is_empty(), "once");
        // The memory has the value, and it was not sent back to the plugin.
        assert_eq!(session.memory.insert(gain).unwrap().param(7), Some(0.25));
        assert!(close(level(&mut session), dry * 0.25));
        assert!(changes(&mut session, gain).unwrap().is_empty());

        // Undo is the host's: the plugin goes back to where it was.
        while session.memory.insert(gain).unwrap().param(7).is_some() {
            assert!(session::undo(&mut session).unwrap());
        }
        assert!(close(level(&mut session), dry));

        // Never asked: a save takes the value in all the same.
        test_plugin::turn_gain_inside(0.5);
        assert!(close(level(&mut session), dry * 0.5));
        session::save(&mut session).unwrap();
        assert_eq!(session.memory.insert(gain).unwrap().param(7), Some(0.5));
        assert!(changes(&mut session, InsertId::from_raw(999)).is_err());
        let dir = session.folder.clone();
        drop(session);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bounce_runs_a_copy_of_the_plugin_in_its_present_state() {
        let (mut session, vox, counters) = rig("plugin-bounce");
        let dry = level(&mut session);
        let gain = insert::add(&mut session, vox, Effect::Plugin(gain_ref())).unwrap();
        param(&mut session, gain, GAIN, 0.25).unwrap();
        assert!(close(level(&mut session), dry * 0.25));
        let path = session.folder.join("bounces").join("mix.wav");
        let mix = export::bounce(&mut session, &path, export::Bounce::default()).unwrap();
        let heard = mix.channel(0)[640..1_000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(close(heard, dry * 0.25), "{heard}");
        assert_eq!(
            counters.loads.load(Ordering::SeqCst),
            2,
            "a second instance"
        );
        assert_eq!(
            counters.live.load(Ordering::SeqCst),
            1,
            "freed after the bounce"
        );
        // The running one was not touched.
        assert!(close(level(&mut session), dry * 0.25));
        let _ = std::fs::remove_dir_all(&session.folder);
    }
}
