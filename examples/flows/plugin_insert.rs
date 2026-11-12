//! `plugin` finds the plugins on this machine and works the ones on your
//! tracks.
//!
//! ```text
//! plugin::scan / params / param / value / missing
//! ```
//!
//! ```text
//! let found = plugin::scan();
//! let verb = insert::add(&mut session, vox, Effect::Plugin(PluginRef::from(&found[0])))?;
//! plugin::param(&mut session, verb, id, 0.3)?;
//! ```
//!
//! ```text
//! cargo run --example plugin_insert
//! ```
//!
//! Uses the first CLAP effect installed; says so and stops when there is
//! none.

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::{Effect, PluginRef, TrackKind};
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, insert, plugin, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let folder = std::env::temp_dir().join("plugin_insert.superb");
    let _ = std::fs::remove_dir_all(&folder);
    let mut session = session::create(&folder, rate, 256)?;

    // One track playing a second of 220 Hz.
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize);
    for ch in 0..2 {
        for (i, s) in tone.channel_mut(ch).iter_mut().enumerate() {
            *s = 0.5 * (core::f32::consts::TAU * 220.0 * i as f32 / rate.hz() as f32).sin();
        }
    }
    session.audio.insert("audio/tone.wav", tone);
    clip::add(&mut session, vox, "audio/tone.wav", TimePosition::ZERO)?;

    let found = plugin::scan();
    println!("{} CLAP plugin(s) installed", found.len());
    // The first effect that loads and fits a stereo track.
    let mut placed = None;
    for candidate in found.iter().filter(|p| p.is_audio_effect()) {
        match insert::add(
            &mut session,
            vox,
            Effect::Plugin(PluginRef::from(candidate)),
        ) {
            Ok(id) => {
                placed = Some((id, candidate));
                break;
            }
            Err(error) => println!("skipping {}: {error}", candidate.name),
        }
    }
    let Some((fx, descriptor)) = placed else {
        println!("no CLAP effect to put on the track; install one and run this again");
        let _ = std::fs::remove_dir_all(&folder);
        return Ok(());
    };
    println!("{} by {} is on vox", descriptor.name, descriptor.vendor);

    let params = plugin::params(&mut session, fx)?;
    for p in params.iter().take(8) {
        println!(
            "  {:>10}  {:<24} {}..{} (default {})",
            p.id, p.name, p.min, p.max, p.default
        );
    }

    transport::start(&mut session)?;
    let before = session::steps::render_blocks(&mut session, 20)?.peak();

    // A parameter is heard at the plugin's next block; a drag is one undo step.
    if let Some(p) = params.iter().find(|p| !p.readonly && !p.hidden) {
        plugin::param(&mut session, fx, p.id, p.max)?;
        session::steps::render_blocks(&mut session, 4)?;
        println!(
            "{} set to {}",
            p.name,
            plugin::value(&mut session, fx, p.id)?
        );
    }

    // A rebuild of the mixer keeps the plugin running.
    track::add(&mut session, "gtr", TrackKind::Audio)?;
    let after = session::steps::render_blocks(&mut session, 20)?.peak();
    println!("peak {before:.3} before, {after:.3} with the parameter moved");

    // Its state is saved with the session and loaded again on open.
    session::save(&mut session)?;
    let file = session::layout(&session).plugin_state(fx);
    let saved = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    println!("saved {saved} bytes of plugin state in {}", file.display());
    drop(session);
    let mut again = session::open(&folder, 256)?;
    session::steps::rebuild(&mut again)?;
    println!(
        "reopened: {} plugin(s) loaded, {} missing",
        again.plugins.len() - plugin::missing(&again).len(),
        plugin::missing(&again).len()
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
