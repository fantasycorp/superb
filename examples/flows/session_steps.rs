//! `session::steps`: one memory, the mixer it builds, one second of audio.
//!
//! The comments are the example. Each one is the block under it: what that
//! code puts in the memory, what `build_mixer` makes of it, and what the
//! offline render then contains.
//!
//! ```text
//! cargo run --example session_steps
//! ```

use std::path::PathBuf;

use superb::primitives::buffers::gain::db_to_linear;
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::cache::AudioCache;
use superb::primitives::io::wav::{self, WavFormat};
use superb::primitives::io::{offline, Engine};
use superb::primitives::mixer::PanLaw;
use superb::primitives::session::{Clip, ClipSource, Memory, MidiNote, TrackKind, TrackSend};
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate, TimeDuration, TimePosition};
use superb::session;

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    // A memory named "session-steps" at 48 kHz, tempo 120. It already holds
    // a master bus. The three tracks below are all `track_count` will report.
    let mut memory = Memory::new("session-steps", rate);

    // vox, an audio track at unity. Half a second of DC at 0.2, held in the
    // cache under the same key the clip names ("tone.wav"). The clip starts
    // at sample 12_000 (0.25 s) and runs for the whole buffer, 24_000
    // samples, so it occupies 12_000..36_000.
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

    // keys, a MIDI track at -3 dB. One A4: MIDI note 69, which
    // `MonoSynthNode::note_to_hz` turns into 440 Hz. Note numbers run
    // `0..=127`, from 8.1757989156 Hz (note 0) to 12543.8539514 Hz (note
    // 127); twelve notes is an octave. Velocity 100, starting at beat 0,
    // held for half a beat. At 120 bpm that is 0.25 s, samples 0..12_000.
    // At 48 kHz one cycle of 440 Hz is 48000/440 samples (about 109.09).
    // The clip is a full second; the note ends at its own length.
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

    // room, a bus at -6 dB. keys feeds it. vox also sends into it at -12 dB,
    // pre-fader, so the send is the raw clip: it skips the vox fader and
    // the vox pan law, and the room bus hears it.
    let room = memory.add_track("room", TrackKind::Bus);
    memory.track_mut(room)?.state.volume_db = -6.0;
    memory.set_track_output(keys, Some(room))?;
    let send = memory.add_send(vox, TrackSend::new(room).level_db(-12.0).pre_fader())?;

    // One strip per track, plus the master. Volume, pan, mute and solo are
    // copied onto the strips; the handles are keyed by these memory ids.
    // Each strip is an input node and a strip node. On top of those eight
    // nodes, `build_mixer` adds the click (silent: the metronome starts
    // off), a player for each clip, and the send. 8 + 1 + 2 + 1 = 12.
    let (mixer, handles) = session::steps::build_mixer(&memory, &cache, 2)?;
    assert_eq!(memory.track_count(), 3);
    assert_eq!(handles.strips.len(), 4);
    assert_eq!(handles.sends.len(), 1);
    assert_eq!(mixer.graph().node_count(), 8 + 1 + 2 + 1);
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

    // The same tempo map, transport already playing, one second in blocks
    // of 256. `offline::render` prepares the engine and pulls the blocks.
    let mut transport = Transport::with_tempo_map(memory.tempo_map().clone());
    transport.play();
    let mut engine = Engine::new(mixer, transport);
    let audio = offline::render(&mut engine, rate, 256, 48_000)?;

    // Both channels match: every pan is centred.
    //
    // 0..12_000 is the A4. Amplitude is velocity/127, then the keys fader
    // (−3 dB), the track's constant-power pan (−3 dB at centre), and the
    // room bus (−6 dB; a bus uses the balance law, unity at centre).
    let centre = PanLaw::ConstantPower.gains(0.0).0;
    let pitch = MonoSynthNode::note_to_hz(69);
    assert!((pitch - 440.0).abs() < 1e-9);
    assert!((MonoSynthNode::note_to_hz(0) - 8.1757989156).abs() < 1e-6);
    assert!((MonoSynthNode::note_to_hz(127) - 12543.8539514).abs() < 1e-4);
    // One cycle of that A4, in samples, at this session's rate.
    assert!((rate.hz() as f64 / pitch - 48_000.0 / 440.0).abs() < 1e-9);
    let note = (100.0 / 127.0) * db_to_linear(-3.0) * centre * db_to_linear(-6.0);
    let note_peak = audio.channel(0)[..12_000]
        .iter()
        .fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        (note_peak - note).abs() < 1e-4,
        "A4 peak {note_peak} vs {note}"
    );
    assert!(
        (audio.channel(1)[..12_000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
            - note)
            .abs()
            < 1e-4
    );

    // 12_000..36_000 is the tone, on both channels. The direct path is 0.2
    // through the vox pan law. The send path is 0.2 through −12 dB and the
    // room bus's −6 dB, with no track pan on it.
    let tone_level = 0.2 * centre + 0.2 * db_to_linear(-12.0) * db_to_linear(-6.0);
    for at in [12_000usize, 24_000, 35_999] {
        assert!(
            (audio.channel(0)[at] - tone_level).abs() < 1e-4,
            "tone L frame {at}: {} vs {tone_level}",
            audio.channel(0)[at]
        );
        assert!(
            (audio.channel(1)[at] - tone_level).abs() < 1e-4,
            "tone R frame {at}: {} vs {tone_level}",
            audio.channel(1)[at]
        );
    }

    // 36_000..48_000 is past the note and past the clip.
    assert!(audio.channel(0)[36_000..].iter().all(|&s| s == 0.0));
    assert!(audio.channel(1)[36_000..].iter().all(|&s| s == 0.0));

    println!(
        "0.00..0.25 s  A4 peak {note_peak:.3}\n\
         0.25..0.75 s  tone {tone_level:.3}\n\
         0.75..1.00 s  silent"
    );

    let path = std::env::temp_dir().join("session_steps.wav");
    wav::write(&path, &audio, rate, WavFormat::Float32)?;
    println!("wrote {}", path.display());
    Ok(())
}
