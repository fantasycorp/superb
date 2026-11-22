//! Inserts: the effects in a track's signal path, in order.
//!
//! An insert sits on a track, a bus or the master, between its input and
//! its fader. The effects built into the crate are a compressor, a
//! limiter, a gate, a de-esser and a leveler. Adding, removing and
//! reordering change the mixer's shape, so it rebuilds. A parameter is an
//! edit: into the memory through the undo history, coalescing, and onto
//! the running effect at once. Bypass is state, like mute: saved, not an
//! undo step; the mixer is rebuilt without the insert.
//!
//! A plugin (`Effect::Plugin`) is an insert like the others: loaded when it
//! is added, kept running across rebuilds, saved with the session. Its
//! parameters are its own, and are reached through the
//! [`plugin`](crate::plugin) flow.
//!
//! Made of `superb::primitives::session::{Insert, Effect, EffectParam}` and
//! its commands (the memory side) and `superb::primitives::graph::dynamics`
//! (the nodes and their live controls).
//!
//! ```
//! use superb::primitives::buffers::PlanarBuffer;
//! use superb::primitives::session::{Effect, EffectParam, TrackKind};
//! use superb::primitives::{SampleRate, TimePosition};
//! use superb::{clip, insert, session, track, transport};
//!
//! let folder = std::env::temp_dir().join(format!("superb-insert-example-{}", std::process::id()));
//! let mut session = session::create(&folder, SampleRate::HZ_48K, 64)?;
//! let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
//! let mut tone = PlanarBuffer::new(2, 48_000);
//! tone.fill(0.5);
//! session.audio.insert("audio/tone.wav", tone);
//! clip::add(&mut session, vox, "audio/tone.wav", TimePosition::ZERO)?;
//! transport::start(&mut session)?;
//! let dry = session::steps::render_blocks(&mut session, 100)?.peak();
//!
//! let comp = insert::add(&mut session, vox, Effect::Compressor)?;
//! insert::param(&mut session, comp, EffectParam::Threshold, -30.0)?;
//! insert::param(&mut session, comp, EffectParam::Ratio, 8.0)?;
//! transport::rewind(&mut session)?;
//! let squeezed = session::steps::render_blocks(&mut session, 100)?;
//! let tail = squeezed.channel(0)[6_000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
//! assert!(tail < dry * 0.5, "{tail} against {dry}");
//! assert!(insert::reduction(&session, comp).unwrap() > 6.0);
//!
//! insert::bypass(&mut session, comp, true)?;
//! transport::rewind(&mut session)?;
//! let through = session::steps::render_blocks(&mut session, 100)?.peak();
//! assert!((through - dry).abs() < 1e-4);
//! assert_eq!(insert::reduction(&session, comp), None);
//! let _ = std::fs::remove_dir_all(&folder);
//! # Ok::<(), superb::primitives::SuperbError>(())
//! ```

use crate::primitives::session::commands::{self, AddInsert, MoveInsert, RemoveInsert};
use crate::primitives::session::{Effect, EffectParam, Insert};
use crate::primitives::{InsertId, Result, SuperbError, TrackId};
use crate::session::Session;

/// Adds an effect at the end of a track's insert chain: the signal runs
/// through the chain in order, then into the fader. Any track takes
/// inserts, buses and the master included. The effect starts at its
/// defaults; set its parameters with [`param`], or a plugin's with
/// [`plugin::param`](crate::plugin::param). A plugin
/// (`Effect::Plugin`) is loaded here, and refused if it cannot be.
///
/// Made of: `Memory::allocate_insert_id` → `Insert::with_id` → for a
/// plugin, [`Session::plugin_loader`] → `AddInsert` through
/// `session::structural`; rebuilds the mixer.
pub fn add(session: &mut Session, track: TrackId, effect: Effect) -> Result<InsertId> {
    let id = session.memory.allocate_insert_id();
    let insert = Insert::with_id(id, effect);
    if matches!(insert.effect, Effect::Plugin(_)) {
        crate::session::plugins::load_new(session, &insert)?;
    }
    if let Err(error) = crate::session::structural(session, AddInsert::new(track, insert)) {
        session.plugins.retain(|(loaded, _)| *loaded != id);
        return Err(error);
    }
    Ok(id)
}

/// Removes an insert. Undo puts it back where it was, parameters and all;
/// a plugin is loaded again in the state it was removed in.
///
/// Made of: for a plugin, its state into `Insert::preset` → `RemoveInsert`
/// through `session::structural`; rebuilds the mixer.
pub fn remove(session: &mut Session, insert: InsertId) -> Result<()> {
    // A plugin's state goes into the memory first, so undo brings it back
    // as it was.
    crate::session::plugins::store_state(session, Some(insert));
    crate::session::structural(session, RemoveInsert::new(insert))
}

/// Moves an insert to `index` in its track's chain; `0` is first.
///
/// Made of: `MoveInsert` through `session::structural`; rebuilds the mixer.
pub fn move_to(session: &mut Session, insert: InsertId, index: usize) -> Result<()> {
    crate::session::structural(session, MoveInsert::new(insert, index))
}

/// Sets one parameter of an effect, in the unit its [`EffectParam`] names
/// (dB, milliseconds, Hz). Heard at the next block. Moves within
/// [`Session::COALESCE_WINDOW`] of each other are one undo step.
///
/// Made of: `Effect::has` → `set_insert_param` through `session::edit` →
/// `InsertParams::set` on the running effect.
pub fn param(
    session: &mut Session,
    insert: InsertId,
    param: EffectParam,
    value: f32,
) -> Result<()> {
    if !value.is_finite() {
        return Err(SuperbError::InvalidArgument { what: "value" });
    }
    if !session.memory.insert(insert)?.effect.has(param) {
        return Err(SuperbError::InvalidArgument {
            what: "the effect has no such parameter",
        });
    }
    crate::session::edit(
        session,
        commands::set_insert_param(insert, param.id(), f64::from(value)),
    )?;
    if let Some(live) = session.mixer.insert(insert) {
        live.set(param, value)?;
    }
    Ok(())
}

/// Bypasses an insert or puts it back in circuit. State, like mute: saved
/// with the session, not an undo step. A built-in effect rebuilds the
/// mixer, which is built without it; a plugin stays loaded and is switched
/// at the next block.
///
/// Made of: `InsertState::bypass` through `session::set_state` → rebuild,
/// or for a plugin `PluginHandle::bypass` on the running node.
pub fn bypass(session: &mut Session, insert: InsertId, on: bool) -> Result<()> {
    crate::session::set_state(session, |m| {
        m.insert_mut(insert)?.state.bypass = on;
        Ok(())
    })?;
    if matches!(session.memory.insert(insert)?.effect, Effect::Plugin(_)) {
        // A plugin stays in the mixer, so its instance is kept; its node
        // passes the signal through while the switch is set.
        if let Some(live) = session.mixer.plugin(insert) {
            live.bypass.store(on, core::sync::atomic::Ordering::Relaxed);
        }
        return Ok(());
    }
    crate::session::changed_structure(session)
}

/// How much an effect was turning the signal down at the end of the last
/// block, in dB, for a gain-reduction meter. `None` for a plugin insert,
/// which reports no reduction, and for a built-in effect that is not in
/// the mixer: bypassed, or added since the last rebuild.
///
/// Made of: `MixerHandles::insert` → `InsertParams::reduction_db`.
pub fn reduction(session: &Session, insert: InsertId) -> Option<f32> {
    session.mixer.insert(insert).map(|p| p.reduction_db())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::buffers::PlanarBuffer;
    use crate::primitives::plugins::PluginFormat;
    use crate::primitives::session::{PluginRef, TrackKind};
    use crate::primitives::{SampleRate, TimePosition};
    use crate::{session, track, transport};

    const R: SampleRate = SampleRate::HZ_48K;

    /// One audio track playing a constant 0.5 for a second.
    fn tone_rig(tag: &str) -> (Session, TrackId) {
        let mut session = session::create(session::scratch(tag), R, 64).unwrap();
        let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
        let mut tone = PlanarBuffer::new(2, 48_000);
        tone.fill(0.5);
        session.audio.insert("tone.wav", tone);
        crate::clip::add(&mut session, vox, "tone.wav", TimePosition::ZERO).unwrap();
        (session, vox)
    }

    /// The level once the effects have settled, from the top.
    fn settled(session: &mut Session) -> f32 {
        transport::rewind(session).unwrap();
        transport::start(session).unwrap();
        let audio = session::steps::render_blocks(session, 200).unwrap();
        audio.channel(0)[10_000..]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn a_stored_insert_is_heard_after_the_rebuild_and_follows_its_parameters() {
        let (mut session, vox) = tone_rig("insert-heard");
        let dry = settled(&mut session);
        let comp = add(&mut session, vox, Effect::Compressor).unwrap();
        assert!(session.needs_rebuild);
        let default = settled(&mut session);
        assert!(session.mixer.insert(comp).is_some());
        // 0.354 on the strip is -9 dBFS: above the default -18 dB threshold.
        assert!(default < dry * 0.9, "{default} against {dry}");
        // A parameter reaches the running effect with no rebuild.
        param(&mut session, comp, EffectParam::Threshold, -40.0).unwrap();
        param(&mut session, comp, EffectParam::Ratio, 20.0).unwrap();
        assert!(!session.needs_rebuild);
        let hard = session::steps::render_blocks(&mut session, 200).unwrap();
        let hard = hard.channel(0)[10_000..]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(hard < default * 0.5, "{hard} against {default}");
        assert!(reduction(&session, comp).unwrap() > 12.0);
        // And survives a rebuild, because the memory holds it.
        session::steps::rebuild(&mut session).unwrap();
        let rebuilt = settled(&mut session);
        assert!((rebuilt - hard).abs() < 0.01, "{rebuilt} against {hard}");
    }

    #[test]
    fn parameters_coalesce_and_undo_and_the_wrong_one_is_refused() {
        let (mut session, vox) = tone_rig("insert-undo");
        let gate = add(&mut session, vox, Effect::Gate).unwrap();
        let steps = session.history.undo_len();
        param(&mut session, gate, EffectParam::Threshold, -50.0).unwrap();
        param(&mut session, gate, EffectParam::Threshold, -60.0).unwrap();
        assert_eq!(session.history.undo_len(), steps + 1, "one drag");
        param(&mut session, gate, EffectParam::Hold, 20.0).unwrap();
        assert_eq!(session.history.undo_len(), steps + 2);
        let stored = |s: &Session| {
            s.memory
                .insert(gate)
                .unwrap()
                .param(EffectParam::Threshold.id())
        };
        assert_eq!(stored(&session), Some(-60.0));
        assert!(matches!(
            param(&mut session, gate, EffectParam::Makeup, 3.0),
            Err(SuperbError::InvalidArgument { .. })
        ));
        assert!(param(&mut session, gate, EffectParam::Threshold, f32::NAN).is_err());
        assert!(param(
            &mut session,
            InsertId::from_raw(999),
            EffectParam::Threshold,
            0.0
        )
        .is_err());
        session::undo(&mut session).unwrap();
        session::undo(&mut session).unwrap();
        assert_eq!(stored(&session), None, "back to the default");
        session::undo(&mut session).unwrap();
        assert!(session.memory.insert(gate).is_err(), "the add is undone");
        session::redo(&mut session).unwrap();
        assert!(session.memory.insert(gate).is_ok());
    }

    #[test]
    fn bypass_is_state_and_takes_the_insert_out_of_the_mixer() {
        let (mut session, vox) = tone_rig("insert-bypass");
        let dry = settled(&mut session);
        let comp = add(&mut session, vox, Effect::Compressor).unwrap();
        param(&mut session, comp, EffectParam::Threshold, -40.0).unwrap();
        assert!(settled(&mut session) < dry * 0.8);
        let steps = session.history.undo_len();
        bypass(&mut session, comp, true).unwrap();
        assert_eq!(session.history.undo_len(), steps, "state, not an edit");
        assert!((settled(&mut session) - dry).abs() < 1e-4);
        assert_eq!(reduction(&session, comp), None);
        // Saved in the state file, and read back on open.
        let dir = session.folder.clone();
        session::state_file(&mut session, Some(dir.join("state.json"))).unwrap();
        session::save(&mut session).unwrap();
        let mut again = session::open(&dir, 64).unwrap();
        assert!(!again.memory.insert(comp).unwrap().state.bypass);
        session::state_file(&mut again, Some(dir.join("state.json"))).unwrap();
        assert!(again.memory.insert(comp).unwrap().state.bypass);
        assert_eq!(
            again
                .memory
                .insert(comp)
                .unwrap()
                .param(EffectParam::Threshold.id()),
            Some(-40.0)
        );
        bypass(&mut session, comp, false).unwrap();
        assert!(settled(&mut session) < dry * 0.8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_chain_runs_in_order_on_buses_and_the_master_too() {
        let (mut session, vox) = tone_rig("insert-order");
        let master = session.memory.master().id();
        let limiter = add(&mut session, master, Effect::Limiter).unwrap();
        param(&mut session, limiter, EffectParam::Ceiling, -20.0).unwrap();
        let limited = settled(&mut session);
        assert!(limited <= 0.1 + 1e-3, "under -20 dBTP: {limited}");
        remove(&mut session, limiter).unwrap();

        let comp = add(&mut session, vox, Effect::Compressor).unwrap();
        let gate = add(&mut session, vox, Effect::Gate).unwrap();
        let chain = |s: &Session| -> Vec<InsertId> {
            s.memory
                .track(vox)
                .unwrap()
                .inserts()
                .iter()
                .map(Insert::id)
                .collect()
        };
        assert_eq!(chain(&session), [comp, gate]);
        move_to(&mut session, gate, 0).unwrap();
        assert_eq!(chain(&session), [gate, comp]);
        assert!(move_to(&mut session, gate, 5).is_err());
        session::undo(&mut session).unwrap();
        assert_eq!(chain(&session), [comp, gate]);
        // A gate that opens above the signal silences what the compressor passes.
        param(&mut session, gate, EffectParam::Threshold, -3.0).unwrap();
        assert!(settled(&mut session) < 0.01);
    }

    #[test]
    fn a_plugin_that_cannot_be_loaded_is_refused() {
        let (mut session, vox) = tone_rig("insert-plugin");
        let added = add(
            &mut session,
            vox,
            Effect::Plugin(PluginRef {
                format: PluginFormat::Clap,
                path: "/plugins/verb.clap".into(),
                id: "com.example.verb".into(),
            }),
        );
        assert!(added.is_err());
        assert_eq!(session.memory.insert_count(), 0);
        assert!(session.plugins.is_empty());
    }
}
