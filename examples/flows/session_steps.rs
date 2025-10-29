//! `session::steps` is four functions. They are the engine side of
//! `session` — the verbs (`track::add`, `clip::add`) call them; you can
//! call them too.
//!
//! ```text
//! rebuild(session)                        from Memory, build a mixer, swap it into the engine
//! render_blocks(session, n) -> audio      run n blocks with no sound card
//! build_mixer(&memory, &audio, channels)  just the mixer; no Session
//! publish(&mut engine, mixer)             just the swap
//! ```
//!
//! ```text
//! use superb::primitives::buffers::PlanarBuffer;
//! use superb::primitives::session::TrackKind;
//! use superb::primitives::{SampleRate, TimePosition};
//! use superb::{clip, session, track, transport};
//!
//! fn main() -> superb::primitives::Result<()> {
//!     let folder = std::env::temp_dir().join("steps");
//!     let mut session = session::create(&folder, SampleRate::HZ_48K, 256)?;
//!
//!     let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
//!     let mut tone = PlanarBuffer::new(2, 48_000);
//!     tone.fill(0.5);
//!     session.audio.insert("tone.wav", tone);
//!     clip::add(&mut session, vox, "tone.wav", TimePosition::ZERO)?;
//!
//!     transport::start(&mut session)?;
//!     let heard = session::steps::render_blocks(&mut session, 8)?;
//!     println!("{}", heard.peak());
//!     Ok(())
//! }
//! ```
//!
//! This file uses `build_mixer` with no Session.
//!
//! ```text
//! cargo run --example session_steps
//! ```

use std::path::PathBuf;

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::cache::AudioCache;
use superb::primitives::io::wav::{self, WavFormat};
use superb::primitives::io::{offline, Engine};
use superb::primitives::session::{Clip, ClipSource, Memory, MidiNote, TrackKind, TrackSend};
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate, TimeDuration, TimePosition};
use superb::session;

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    // 48 kHz, tempo 120. Starts with a master bus.
    let mut memory = Memory::new("session-steps", rate);

    // Audio track. 0.5 s of 0.2, cached as "tone.wav". Clip at 0.25 s.
    let vox = memory.add_track("vox", TrackKind::Audio);
    let mut tone = PlanarBuffer::new(2, 24_000);
    tone.fill(0.2);
    let mut cache = AudioCache::new();
    cache.insert("tone.wav", tone);
    memory.add_clip(
        vox,
        Clip::new(
            "take",
            TimePosition::from_samples(12_000),
            TimeDuration::from_samples(24_000),
            ClipSource::Audio {
                path: PathBuf::from("tone.wav"),
                offset: TimeDuration::ZERO,
            },
        )?,
    )?;

    // MIDI track at -3 dB. One note 69 from beat 0 for 0.5 beats. Clip is 1 s.
    let keys = memory.add_track("keys", TrackKind::Midi);
    memory.track_mut(keys)?.state.volume_db = -3.0;
    memory.add_clip(
        keys,
        Clip::new(
            "a4",
            TimePosition::ZERO,
            TimeDuration::from_samples(48_000),
            ClipSource::Midi {
                notes: vec![MidiNote {
                    beat: 0.0,
                    length: 0.5,
                    note: 69,
                    velocity: 100,
                    channel: 0,
                }],
            },
        )?,
    )?;

    // Bus at -6 dB. keys routes into it. vox sends into it at -12 dB, pre-fader.
    let room = memory.add_track("room", TrackKind::Bus);
    memory.track_mut(room)?.state.volume_db = -6.0;
    memory.set_track_output(keys, Some(room))?;
    let send = memory.add_send(vox, TrackSend::new(room).level_db(-12.0).pre_fader())?;

    let (mixer, handles) = session::steps::build_mixer(&memory, &cache, 2)?;
    println!(
        "{} tracks became {} strips, {} send(s); the graph has {} nodes",
        memory.track_count(),
        handles.strips.len(),
        handles.sends.len(),
        mixer.graph().node_count()
    );
    for (track, strip) in &handles.strips {
        println!(
            "  {} at {:.1} dB{}",
            memory.track(*track)?.name,
            strip.volume_db(),
            if strip.is_muted() { " (muted)" } else { "" }
        );
    }
    println!(
        "  vox send at {:.1} dB, on: {}",
        handles.send(send).map_or(0.0, |s| s.level_db()),
        handles.send(send).is_some_and(|s| s.is_enabled())
    );

    // Playing; render 1 s in blocks of 256.
    let mut transport = Transport::with_tempo_map(memory.tempo_map().clone());
    transport.play();
    let mut engine = Engine::new(mixer, transport);
    let audio = offline::render(&mut engine, rate, 256, 48_000)?;
    println!(
        "rendered {} frames, peak {:.3}",
        audio.frames(),
        audio.peak()
    );

    let path = std::env::temp_dir().join("session_steps.wav");
    wav::write(&path, &audio, rate, WavFormat::Float32)?;
    println!("wrote {}", path.display());
    Ok(())
}
