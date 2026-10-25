//! `session`, `track`, `clip`: a folder at 48 kHz with two tracks, two clips,
//! an undo, and a save that opens again.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example new_session
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::graph::nodes::MonoSynthNode;
use superb::primitives::session::{ClipSource, MidiNote, TrackKind};
use superb::primitives::{BeatPosition, Result, SampleRate, TimePosition};
use superb::{clip, marker, session, track};

fn main() -> Result<()> {
    // A folder named after itself, holding `memory.json` and `audio/`.
    // 48 kHz, blocks of 256. The tempo is 120. `track_count` is 0.
    let folder = std::env::temp_dir().join("My first session");
    let mut session = session::create(&folder, SampleRate::HZ_48K, 256)?;
    let rate = session.memory.sample_rate();
    assert_eq!(rate, SampleRate::HZ_48K);
    assert_eq!(session.memory.name(), "My first session");
    assert_eq!(session.memory.track_count(), 0);
    assert!((session.memory.tempo_map().tempo_at(BeatPosition::ZERO) - 120.0).abs() < 1e-9);

    // vox is an audio track, keys a MIDI track. `track_count` is 2.
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;
    assert_eq!(session.memory.track_count(), 2);
    assert_eq!(session.memory.track(vox)?.kind, TrackKind::Audio);
    assert_eq!(session.memory.track(keys)?.kind, TrackKind::Midi);

    // One second of a 220 Hz sine at amplitude 0.3, stereo, stored under
    // "audio/vox-take-1.wav": 48_000 frames. Sample 100 of that formula is
    // what both channels hold.
    let mut take = PlanarBuffer::new(2, rate.hz() as usize);
    for ch in 0..2 {
        for (i, s) in take.channel_mut(ch).iter_mut().enumerate() {
            let t = i as f32 / rate.hz() as f32;
            *s = 0.3 * (t * 220.0 * core::f32::consts::TAU).sin();
        }
    }
    session.audio.insert("audio/vox-take-1.wav", take);
    let stored = session
        .audio
        .get(std::path::Path::new("audio/vox-take-1.wav"))
        .expect("vox take");
    assert_eq!(stored.frames(), 48_000);
    assert_eq!(stored.channels(), 2);
    let i = 100usize;
    let t = i as f32 / rate.hz() as f32;
    let sample = 0.3 * (t * 220.0 * core::f32::consts::TAU).sin();
    assert!((stored.channel(0)[i] - sample).abs() < 1e-6);
    assert!((stored.channel(1)[i] - sample).abs() < 1e-6);
    // The clip starts at 0.5 s (sample 24_000), runs the whole buffer
    // (48_000 samples) and is named from the key's stem, "vox-take-1".
    let clip = clip::add(
        &mut session,
        vox,
        "audio/vox-take-1.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;
    let placed = session.memory.clip(clip)?;
    assert_eq!(placed.name, "vox-take-1");
    assert_eq!(placed.start().samples(), 24_000);
    assert_eq!(placed.length().samples(), 48_000);

    // Four notes, a beat apart, each 0.9 beats long, velocity 96, channel 0:
    // C4, E4, G4, C5. `note_to_hz` is `440 * 2^((note - 69) / 12)`.
    // The last note ends at beat 3.9. At the session's 120 bpm that is
    // 3.9 × 24_000 = 93_600 samples (1.95 s). The clip is named "keys".
    let notes: Vec<MidiNote> = [60u8, 64, 67, 72]
        .iter()
        .enumerate()
        .map(|(i, &note)| MidiNote {
            beat: i as f64,
            length: 0.9,
            note,
            velocity: 96,
            channel: 0,
        })
        .collect();
    assert!((MonoSynthNode::note_to_hz(60) - 261.6255653006).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(64) - 329.6275569129).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(67) - 391.9954359817).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(72) - 523.2511306012).abs() < 1e-6);
    clip::add(&mut session, keys, notes, TimePosition::ZERO)?;
    let midi = &session.memory.track(keys)?.clips()[0];
    assert_eq!(midi.name, "keys");
    assert_eq!(midi.start().samples(), 0);
    assert_eq!(midi.length().samples(), 93_600);
    match &midi.source {
        ClipSource::Midi { notes } => {
            assert_eq!(
                notes
                    .iter()
                    .map(|n| (n.note, n.beat, n.velocity, n.channel))
                    .collect::<Vec<_>>(),
                vec![
                    (60, 0.0, 96, 0),
                    (64, 1.0, 96, 0),
                    (67, 2.0, 96, 0),
                    (72, 3.0, 96, 0)
                ]
            );
            assert!(notes.iter().all(|n| (n.length - 0.9).abs() < 1e-9));
        }
        _ => panic!("keys holds MIDI notes"),
    }

    // Tempo becomes 100. The keys clip stays 93_600 samples. A marker named
    // "verse" sits at 0.5 s (sample 24_000). Counts: 2 tracks, 2 clips.
    session::set_tempo(&mut session, 100.0)?;
    marker::add(&mut session, "verse", TimePosition::from_seconds(0.5, rate))?;
    assert!((session.memory.tempo_map().tempo_at(BeatPosition::ZERO) - 100.0).abs() < 1e-9);
    assert_eq!(
        session.memory.track(keys)?.clips()[0].length().samples(),
        93_600
    );
    assert_eq!(session.memory.markers().len(), 1);
    assert_eq!(session.memory.markers()[0].name, "verse");
    assert_eq!(session.memory.markers()[0].position().samples(), 24_000);
    assert_eq!(session.memory.track_count(), 2);
    assert_eq!(session.memory.clip_count(), 2);
    println!(
        "{}: {} tracks, {} clips, {} bpm",
        session.memory.name(),
        session.memory.track_count(),
        session.memory.clip_count(),
        session.memory.tempo_map().tempo_at(BeatPosition::ZERO)
    );

    // The vox clip moves to 2.0 s (sample 96_000). `seal` closes that undo
    // step. Undo puts it back at sample 24_000; redo returns it to 96_000.
    clip::move_to(
        &mut session,
        clip,
        vox,
        TimePosition::from_seconds(2.0, rate),
    )?;
    session.history.seal();
    assert_eq!(session.memory.clip(clip)?.start().samples(), 96_000);
    println!(
        "clip now starts at {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );
    session::undo(&mut session)?;
    assert_eq!(session.memory.clip(clip)?.start().samples(), 24_000);
    println!(
        "after undo:      {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );
    session::redo(&mut session)?;
    assert_eq!(session.memory.clip(clip)?.start().samples(), 96_000);
    println!(
        "after redo:      {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );

    // The history is dirty, then `save` writes `memory.json` and
    // `audio/vox-take-1.wav` and marks it clean.
    assert!(!session.history.is_clean());
    println!("saved? {}", session.history.is_clean());
    session::save(&mut session)?;
    assert!(session.history.is_clean());
    assert!(folder.join("memory.json").is_file());
    assert!(folder.join("audio/vox-take-1.wav").is_file());
    println!(
        "saved? {} -> {}",
        session.history.is_clean(),
        folder.display()
    );

    // Open reads both back: the same name, 2 tracks, 2 clips, 1 audio
    // buffer, the vox clip still at sample 96_000, the marker still at
    // sample 24_000.
    let again = session::open(&folder, 256)?;
    assert_eq!(again.memory.name(), "My first session");
    assert_eq!(again.memory.track_count(), 2);
    assert_eq!(again.memory.clip_count(), 2);
    assert_eq!(again.audio.len(), 1);
    assert_eq!(again.memory.markers()[0].name, "verse");
    assert_eq!(again.memory.markers()[0].position().samples(), 24_000);
    let vox_again = again
        .memory
        .tracks()
        .iter()
        .find(|t| t.name == "vox")
        .expect("vox");
    assert_eq!(vox_again.clips()[0].start().samples(), 96_000);
    assert_eq!(vox_again.clips()[0].length().samples(), 48_000);
    println!(
        "reopened {:?}: {} tracks, {} clips, {} audio file(s), marker {:?} at {:.2} s",
        again.memory.name(),
        again.memory.track_count(),
        again.memory.clip_count(),
        again.audio.len(),
        again.memory.markers()[0].name,
        again.memory.markers()[0].position().to_seconds(rate)
    );
    Ok(())
}
