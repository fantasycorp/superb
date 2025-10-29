//! `session` is create, open, save, tempo, undo. `track::add` and
//! `clip::add` put tracks and clips in the Memory.
//!
//! ```text
//! session::create / open / save / set_tempo / undo / redo
//! track::add
//! clip::add / move_to
//! marker::add
//! ```
//!
//! ```text
//! let mut session = session::create(&folder, SampleRate::HZ_48K, 256)?;
//! let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
//! clip::add(&mut session, vox, "take.wav", TimePosition::ZERO)?;
//! session::save(&mut session)?;
//! let again = session::open(&folder, 256)?;
//! ```
//!
//! ```text
//! cargo run --example new_session
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::{MidiNote, TrackKind};
use superb::primitives::{BeatPosition, Result, SampleRate, TimePosition};
use superb::{clip, marker, session, track};

fn main() -> Result<()> {
    // Folder at 48 kHz, blocks of 256. Tempo 120.
    let folder = std::env::temp_dir().join("My first session");
    let mut session = session::create(&folder, SampleRate::HZ_48K, 256)?;
    let rate = session.memory.sample_rate();

    // Audio track and MIDI track.
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;

    // 1 s of 220 Hz sine at 0.3, stereo, keyed "audio/vox-take-1.wav".
    let mut take = PlanarBuffer::new(2, rate.hz() as usize);
    for ch in 0..2 {
        for (i, s) in take.channel_mut(ch).iter_mut().enumerate() {
            let t = i as f32 / rate.hz() as f32;
            *s = 0.3 * (t * 220.0 * core::f32::consts::TAU).sin();
        }
    }
    session.audio.insert("audio/vox-take-1.wav", take);
    // Clip at 0.5 s, named "vox-take-1".
    let clip = clip::add(
        &mut session,
        vox,
        "audio/vox-take-1.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;

    // Four notes a beat apart, each 0.9 beats, velocity 96.
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
    clip::add(&mut session, keys, notes, TimePosition::ZERO)?;

    // Tempo 100. Marker "verse" at 0.5 s.
    session::set_tempo(&mut session, 100.0)?;
    marker::add(&mut session, "verse", TimePosition::from_seconds(0.5, rate))?;
    println!(
        "{}: {} tracks, {} clips, {} bpm",
        session.memory.name(),
        session.memory.track_count(),
        session.memory.clip_count(),
        session.memory.tempo_map().tempo_at(BeatPosition::ZERO)
    );

    // Move the vox clip to 2.0 s. Undo / redo that move.
    clip::move_to(
        &mut session,
        clip,
        vox,
        TimePosition::from_seconds(2.0, rate),
    )?;
    session.history.seal();
    println!(
        "clip now starts at {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );
    session::undo(&mut session)?;
    println!(
        "after undo:      {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );
    session::redo(&mut session)?;
    println!(
        "after redo:      {:.2} s",
        session.memory.clip(clip)?.start().to_seconds(rate)
    );

    // save writes memory.json and audio/vox-take-1.wav.
    println!("saved? {}", session.history.is_clean());
    session::save(&mut session)?;
    println!(
        "saved? {} -> {}",
        session.history.is_clean(),
        folder.display()
    );

    // open reads it back.
    let again = session::open(&folder, 256)?;
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
