//! `midi`: a monitoring track, three notes through one voice, then silence.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example midi_keys
//! cargo run --example midi_keys --features midi-io   # also lists ports
//! ```

use superb::primitives::graph::nodes::MonoSynthNode;
use superb::primitives::midi::{MidiChannel, MidiMessage};
use superb::primitives::mixer::PanLaw;
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate};
use superb::{midi, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("midi_keys.superb"), rate, 128)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;

    // Monitoring puts a `MonoSynthNode` on the keys strip. Before any note
    // the next two blocks (256 frames) are silent. `note_to_hz` is
    // `440 * 2^((note - 69) / 12)`. The centre constant-power pan is √2/2
    // on each side; the master bus passes that at unity.
    track::monitor(&mut session, keys, true)?;
    transport::start(&mut session)?;
    assert!(session.memory.track(keys)?.state.monitor);
    println!("monitoring: {}", session.memory.track(keys)?.state.monitor);
    let before = session::steps::render_blocks(&mut session, 2)?;
    assert!(before.is_silent());
    println!("silence before any note: {}", before.is_silent());
    let centre = PanLaw::ConstantPower.gains(0.0).0;
    assert!((MonoSynthNode::note_to_hz(60) - 261.6255653006).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(64) - 329.6275569129).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(67) - 391.9954359817).abs() < 1e-6);

    // C4, note 60, at 261.6255653006 Hz, velocity 100. Amplitude is
    // 100/127, then the centre pan. Eight blocks (1_024 frames) peak at
    // 0.557.
    midi::play_note(&mut session, 0, 60, 100)?;
    let audio = session::steps::render_blocks(&mut session, 8)?;
    let c4 = 100.0 / 127.0 * centre;
    assert!((audio.peak() - c4).abs() < 1e-3);
    println!("C4 sounding, peak {:.3}", audio.peak());
    // The synth holds one note. E4 (64, 329.6275569129 Hz) replaces C4,
    // then G4 (67, 391.9954359817 Hz, velocity 90) replaces E4. These
    // eight blocks are G4. 90/127 through the same pan peaks at 0.501.
    for note in [64, 67] {
        midi::play_note(&mut session, 0, note, 90)?;
    }
    let audio = session::steps::render_blocks(&mut session, 8)?;
    let g4 = 90.0 / 127.0 * centre;
    assert!((audio.peak() - g4).abs() < 1e-3);
    println!("C major chord, peak {:.3}", audio.peak());

    // Note-off for 60, 64 and 67. The voice that is actually sounding is
    // 67, and it goes silent on that message. The last of the four blocks
    // is all zeros.
    for note in [60, 64, 67] {
        midi::release_note(&mut session, 0, note)?;
    }
    let audio = session::steps::render_blocks(&mut session, 4)?;
    let block = session.engine.max_frames();
    let released = audio.channel(0)[audio.frames() - block..]
        .iter()
        .all(|&s| s == 0.0);
    assert!(released);
    println!("after release, last block silent: {released}");

    // Control change 1, value 64, on channel 0. The synth has no use for
    // it, so the voice stays silent.
    midi::send(
        &mut session,
        MidiMessage::control_change(MidiChannel::new(0)?, 1, 64)?,
    )?;

    // With `midi-io`, every input port is printed by index and name. The
    // first one, when there is one, is connected and held on the session.
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

    // Monitoring off drops the instrument at the next rebuild. Note 60 is
    // queued and the next four blocks stay silent.
    track::monitor(&mut session, keys, false)?;
    midi::play_note(&mut session, 0, 60, 100)?;
    let ignored = session::steps::render_blocks(&mut session, 4)?;
    assert!(ignored.is_silent());
    println!("monitor off, note ignored: {}", ignored.is_silent());
    Ok(())
}
