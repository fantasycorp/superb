//! `metronome` is the click, and the bars it plays before a recording.
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

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::capture::capture_queue;
use superb::primitives::session::TrackKind;
use superb::primitives::transport::TransportState;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{metronome, record, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let block = 256;
    let folder = std::env::temp_dir().join("metronome_count_in.superb");
    let mut session = session::create(&folder, rate, block)?;
    // 120 bpm (a beat is 0.5 s). vox armed.
    session::set_tempo(&mut session, 120.0)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    track::arm(&mut session, vox, true)?;

    // Click at −6 dB. One render per beat, four beats.
    metronome::enable(&mut session, true)?;
    metronome::level(&mut session, -6.0)?;
    transport::start(&mut session)?;
    let blocks_per_beat = rate.hz() as usize / 2 / block;
    for beat in 1..=4 {
        let audio = session::steps::render_blocks(&mut session, blocks_per_beat)?;
        println!("beat {beat}: click peak {:.3}", audio.peak());
    }
    transport::stop(&mut session)?;
    metronome::enable(&mut session, false)?;

    // 1 bar count-in. Seek to 2.0 s. Recording rolls from 0, punches in at 2.0 s.
    metronome::count_in(&mut session, 1)?;
    transport::seek(&mut session, TimePosition::from_seconds(2.0, rate))?;
    session::steps::render_blocks(&mut session, 1)?;
    let (mut mic, blocks) = capture_queue(1, block, 64)?;
    record::start(
        &mut session,
        record::Source::Queue {
            blocks,
            channels: 1,
        },
    )?;
    let recording = session.recording.as_ref().expect("recording");
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
    for n in 0..rendered {
        mic.push(&sung, (n * block) as u64 * 1_000_000 / rate.hz() as u64);
        let heard = session::steps::render_blocks(&mut session, 1)?;
        if transport::state(&session) == TransportState::Recording {
            dropped_in.get_or_insert(transport::playhead(&session).to_seconds(rate));
        } else {
            count_in_peak = count_in_peak.max(heard.peak());
        }
    }
    println!(
        "count-in click peak {count_in_peak:.3}; dropped into record at {:.2} s",
        dropped_in.expect("dropped into record")
    );
    let clips = record::stop(&mut session)?;
    let clip = session.memory.clip(clips[0])?;
    println!(
        "clip \"{}\" starts at {:.2} s and runs {:.2} s",
        clip.name,
        clip.start().to_seconds(rate),
        clip.length().to_seconds(rate)
    );
    Ok(())
}
