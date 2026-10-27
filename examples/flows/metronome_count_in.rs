//! `metronome`: four clicks at −6 dB, then a one-bar count-in into a take.
//!
//! ```text
//! metronome::enable / level / count_in
//! ```
//!
//! ```text
//! metronome::enable(&mut session, true)?;
//! metronome::level(&mut session, -6.0)?;
//! metronome::count_in(&mut session, 1)?;
//! record::start(&mut session, source)?;
//! ```
//!
//! ```text
//! cargo run --example metronome_count_in
//! ```

use superb::primitives::buffers::gain::db_to_linear;
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::capture::capture_queue;
use superb::primitives::session::{ClipSource, TrackKind};
use superb::primitives::transport::TransportState;
use superb::primitives::{BeatPosition, Result, SampleRate, TimePosition};
use superb::{metronome, record, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let block = 256;
    let folder = std::env::temp_dir().join("metronome_count_in.superb");
    let mut session = session::create(&folder, rate, block)?;
    // 120 bpm (a beat is 0.5 s). vox armed.
    session::set_tempo(&mut session, 120.0)?;
    assert!((session.memory.tempo_map().tempo_at(BeatPosition::ZERO) - 120.0).abs() < 1e-9);
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    track::arm(&mut session, vox, true)?;

    // Click at −6 dB. Accent (beat 1) crest at sample 8; other beats at
    // sample 12, 0.7×. 93 blocks per beat. Four beats → sample 95_232.
    metronome::enable(&mut session, true)?;
    metronome::level(&mut session, -6.0)?;
    transport::start(&mut session)?;
    let blocks_per_beat = rate.hz() as usize / 2 / block;
    assert_eq!(blocks_per_beat, 93);
    let decay = (-6.0f32 / 2_400.0).exp();
    let mut accent = db_to_linear(-6.0);
    for _ in 0..8 {
        accent *= decay;
    }
    let mut plain = db_to_linear(-6.0) * 0.7;
    for _ in 0..12 {
        plain *= decay;
    }
    for beat in 1..=4 {
        let audio = session::steps::render_blocks(&mut session, blocks_per_beat)?;
        let expected = if beat == 1 { accent } else { plain };
        assert!(
            (audio.peak() - expected).abs() < 1e-4,
            "beat {beat}: {} vs {expected}",
            audio.peak()
        );
        println!("beat {beat}: click peak {:.3}", audio.peak());
    }
    assert_eq!(transport::playhead(&session).samples(), 95_232);
    transport::stop(&mut session)?;
    metronome::enable(&mut session, false)?;

    // 1 bar count-in. Seek to 2.0 s. Recording rolls from 0, punches in at
    // 96_000. 562 blocks. Clip starts at 2.0 s, 47_872 samples, offset 96_000.
    metronome::count_in(&mut session, 1)?;
    transport::seek(&mut session, TimePosition::from_seconds(2.0, rate))?;
    session::steps::render_blocks(&mut session, 1)?;
    assert_eq!(transport::playhead(&session).samples(), 96_000);
    let (mut mic, blocks) = capture_queue(1, block, 64)?;
    record::start(
        &mut session,
        record::Source::Queue {
            blocks,
            channels: 1,
        },
    )?;
    let recording = session.recording.as_ref().expect("recording");
    assert_eq!(recording.started_at().samples(), 0);
    assert_eq!(recording.punch_in().samples(), 96_000);
    println!(
        "rolling from {:.2} s, recording from {:.2} s",
        recording.started_at().to_seconds(rate),
        recording.punch_in().to_seconds(rate)
    );
    let mut sung = PlanarBuffer::new(1, block);
    sung.fill(0.3);
    let mut dropped_in = None;
    let mut count_in_peak = 0.0f32;
    let rendered = 3 * rate.hz() as usize / block;
    assert_eq!(rendered, 562);
    for n in 0..rendered {
        mic.push(&sung, (n * block) as u64 * 1_000_000 / rate.hz() as u64);
        let heard = session::steps::render_blocks(&mut session, 1)?;
        if transport::state(&session) == TransportState::Recording {
            dropped_in.get_or_insert(transport::playhead(&session).to_seconds(rate));
        } else {
            count_in_peak = count_in_peak.max(heard.peak());
        }
    }
    let dropped = dropped_in.expect("dropped into record");
    assert!((dropped - 2.0).abs() < 1e-9);
    assert!((count_in_peak - accent).abs() < 1e-4);
    println!(
        "count-in click peak {count_in_peak:.3}; dropped into record at {:.2} s",
        dropped
    );
    let clips = record::stop(&mut session)?;
    let clip = session.memory.clip(clips[0])?;
    assert_eq!(clip.start().samples(), 96_000);
    assert_eq!(clip.length().samples(), 47_872);
    match &clip.source {
        ClipSource::Audio { path, offset } => {
            assert_eq!(offset.samples(), 96_000);
            assert_eq!(path.parent().and_then(|p| p.to_str()), Some("audio"));
            assert_eq!(
                clip.name,
                path.file_stem().unwrap().to_string_lossy().as_ref()
            );
            assert!(clip.name.starts_with("vox "));
        }
        _ => panic!("the take is an audio clip"),
    }
    println!(
        "clip \"{}\" starts at {:.2} s and runs {:.2} s",
        clip.name,
        clip.start().to_seconds(rate),
        clip.length().to_seconds(rate)
    );
    Ok(())
}
