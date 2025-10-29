//! `midi` module allows you to interact with midi through play and release actions.
//!
//! ```text
//! midi::play_note / release_note / send
//! midi::connect_input   (midi-io)
//! track::monitor
//! ```
//!
//! ```text
//! track::monitor(&mut session, keys, true)?;
//! midi::play_note(&mut session, 0, 60, 100)?;
//! session::steps::render_blocks(&mut session, 8)?;
//! midi::release_note(&mut session, 0, 60)?;
//! ```
//!
//! ```text
//! cargo run --example midi_keys
//! cargo run --example midi_keys --features midi-io   # also lists ports
//! ```

use superb::primitives::midi::{MidiChannel, MidiMessage};
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate};
use superb::{midi, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("midi_keys.superb"), rate, 128)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;

    // Monitor: a synth on the keys strip.
    track::monitor(&mut session, keys, true)?;
    transport::start(&mut session)?;
    println!("monitoring: {}", session.memory.track(keys)?.state.monitor);
    session::steps::render_blocks(&mut session, 2)?;

    // Note 60. Then 64 and 67: one voice, each replaces the last.
    midi::play_note(&mut session, 0, 60, 100)?;
    let audio = session::steps::render_blocks(&mut session, 8)?;
    println!("note 60, peak {:.3}", audio.peak());
    for note in [64, 67] {
        midi::play_note(&mut session, 0, note, 90)?;
    }
    let audio = session::steps::render_blocks(&mut session, 8)?;
    println!("note 67 sounding, peak {:.3}", audio.peak());

    // Note-off names a note. 67 is the one still sounding.
    for note in [60, 64, 67] {
        midi::release_note(&mut session, 0, note)?;
    }
    session::steps::render_blocks(&mut session, 4)?;
    println!("released 60, 64, 67");

    // CC 1, value 64. The synth has no mapping for it.
    midi::send(
        &mut session,
        MidiMessage::control_change(MidiChannel::new(0)?, 1, 64)?,
    )?;

    // --features midi-io: print every MIDI input the OS sees, then attach
    // ports[0] so notes from that device go into this session.
    #[cfg(feature = "midi-io")]
    {
        let ports = superb::devices::midi_inputs()?;
        if ports.is_empty() {
            println!("no MIDI input ports");
        }
        for p in &ports {
            println!("MIDI in {}: {}", p.index, p.name);
        }
        if !ports.is_empty() {
            midi::connect_input(&mut session, ports[0].index)?;
            println!(
                "connected {} ({} input(s) held by the session)",
                ports[0].name,
                session.midi_inputs.len()
            );
        }
    }

    // Monitor off: notes no longer sound.
    track::monitor(&mut session, keys, false)?;
    midi::play_note(&mut session, 0, 60, 100)?;
    session::steps::render_blocks(&mut session, 4)?;
    println!("monitor off");
    Ok(())
}
