//! `record` records onto armed tracks. `stop` places a clip on each.
//!
//! ```text
//! record::start / stop / collect
//! track::arm / input / monitor
//! ```
//!
//! ```text
//! track::arm(&mut session, vox, true)?;
//! track::input(&mut session, vox, Some(Input::Mono(0)))?;
//! record::start(&mut session, record::Source::Queue { blocks, channels: 1 })?;
//! let clips = record::stop(&mut session)?;
//! ```
//!
//! ```text
//! cargo run --example record_take
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::capture::capture_queue;
use superb::primitives::io::{EngineCommand, LiveInput};
use superb::primitives::session::{Input, TrackKind};
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{record, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let block = 256;
    let folder = std::env::temp_dir().join("record_take.superb");
    let mut session = session::create(&folder, rate, block)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let _bed = track::add(&mut session, "bed", TrackKind::Audio)?;

    // vox armed, input channel 0, mono. bed unarmed.
    track::arm(&mut session, vox, true)?;
    track::input(&mut session, vox, Some(Input::Mono(0)))?;
    println!("vox armed: {}", session.memory.track(vox)?.state.arm);

    // Monitor the live input on vox.
    track::monitor(&mut session, vox, true)?;
    let (mut ear, live) = capture_queue(1, block, 8)?;
    session
        .engine
        .send(EngineCommand::LiveInput(Some(LiveInput::new(
            live, 1, block,
        )?)))?;

    transport::start(&mut session)?;
    session::steps::render_blocks(&mut session, rate.hz() as usize / block)?;
    println!(
        "punching in at {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // Record from the playhead into audio/vox <n>.wav.
    let (mut mic, blocks) = capture_queue(1, block, 64)?;
    record::start(
        &mut session,
        record::Source::Queue {
            blocks,
            channels: 1,
        },
    )?;
    println!(
        "recording into {:?}",
        session.recording.as_ref().map(|r| r.files().to_vec())
    );

    // Half a second of 440 Hz at 0.4, pushed to the mic and the monitor.
    let mut phase = 0.0f32;
    let mut monitored = 0.0f32;
    let mut sung = PlanarBuffer::new(1, block);
    let blocks = rate.hz() as usize / 2 / block;
    for n in 0..blocks {
        for s in sung.channel_mut(0) {
            *s = 0.4 * phase.sin();
            phase += 440.0 * core::f32::consts::TAU / rate.hz() as f32;
        }
        let timestamp_us = (n * block) as u64 * 1_000_000 / rate.hz() as u64;
        mic.push(&sung, timestamp_us);
        ear.push(&sung, timestamp_us);
        let heard = session::steps::render_blocks(&mut session, 1)?;
        monitored = monitored.max(heard.peak());
        if n == blocks / 2 {
            let frames = record::collect(&mut session);
            let r = session.recording.as_ref().expect("recording");
            println!(
                "half way ({:?}): {frames} frames, {} sealed on disk, {} dropped, {} disk errors, peak {:.2}",
                transport::state(&session),
                r.sealed_frames(),
                r.dropped(),
                r.disk_errors(),
                r.peaks()[0]
            );
        }
    }
    println!("heard while recording: peak {monitored:.3} on the vox strip");

    let clips = record::stop(&mut session)?;
    if let Ok(entries) = std::fs::read_dir(folder.join("audio")) {
        for entry in entries.flatten() {
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            println!("written {} ({bytes} bytes)", entry.path().display());
        }
    }
    for id in &clips {
        let clip = session.memory.clip(*id)?;
        println!(
            "clip {:?} on {}: {:.3} s .. {:.3} s (source {:?})",
            clip.name,
            session.memory.track(session.memory.clip_track(*id)?)?.name,
            clip.start().to_seconds(rate),
            (clip.start() + clip.length()).to_seconds(rate),
            clip.source
        );
    }
    println!("undo label: {:?}", session.history.undo_label());

    transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
    transport::start(&mut session)?;
    let heard = session::steps::render_blocks(&mut session, 8)?;
    println!("playback of the clip, peak {:.3}", heard.peak());

    // Undo removes the clip and the file. Buffer stays in session.audio.
    session::undo(&mut session)?;
    println!(
        "after undo: {} clips, {} recording(s) in memory, {} unfinished on disk",
        session.memory.clip_count(),
        session.audio.len(),
        record::unfinished(&session).len()
    );

    // Record 0.5 s from the default input.
    #[cfg(feature = "devices")]
    {
        use superb::devices;
        if devices::inputs()?.is_empty() {
            println!("no input device to record from");
        } else {
            track::arm(&mut session, vox, true)?;
            record::start(&mut session, record::Source::Device(None))?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            let clips = record::stop(&mut session)?;
            println!("recorded {} clip(s) from the default input", clips.len());
        }
    }
    Ok(())
}
