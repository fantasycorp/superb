//! `record`: an armed mono track, a 440 Hz take, a clip, an undo.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example record_take
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::capture::capture_queue;
use superb::primitives::io::{EngineCommand, LiveInput};
use superb::primitives::mixer::PanLaw;
use superb::primitives::session::{ClipSource, Input, TrackKind};
use superb::primitives::transport::TransportState;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{record, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let block = 256;
    let folder = std::env::temp_dir().join("record_take.superb");
    let mut session = session::create(&folder, rate, block)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let _bed = track::add(&mut session, "bed", TrackKind::Audio)?;

    // vox is armed and records input channel 0 as mono. `bed` stays unarmed.
    track::arm(&mut session, vox, true)?;
    track::input(&mut session, vox, Some(Input::Mono(0)))?;
    assert!(session.memory.track(vox)?.state.arm);
    assert_eq!(session.memory.track(vox)?.state.input, Some(Input::Mono(0)));
    assert!(!session.memory.track(_bed)?.state.arm);
    println!("vox armed: {}", session.memory.track(vox)?.state.arm);

    // Monitoring plays the live input on both sides of the vox strip.
    // The queue below is that input: 1 channel, blocks of 256, 8 queued.
    track::monitor(&mut session, vox, true)?;
    let (mut ear, live) = capture_queue(1, block, 8)?;
    session
        .engine
        .send(EngineCommand::LiveInput(Some(LiveInput::new(
            live, 1, block,
        )?)))?;

    // 48_000 / 256 = 187 blocks. The playhead is sample 47_872, 0.997 s,
    // which prints as 0.997 s.
    transport::start(&mut session)?;
    session::steps::render_blocks(&mut session, rate.hz() as usize / block)?;
    assert_eq!(transport::playhead(&session).samples(), 47_872);
    println!(
        "punching in at {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // The take's queue: 1 channel, 64 blocks waiting. Recording starts at
    // the playhead. The file is the next free `audio/vox <n>.wav` (a clean
    // folder uses `audio/vox 1.wav`); the clip takes that stem as its name.
    let (mut mic, blocks) = capture_queue(1, block, 64)?;
    record::start(
        &mut session,
        record::Source::Queue {
            blocks,
            channels: 1,
        },
    )?;
    let take_key = session
        .recording
        .as_ref()
        .expect("recording")
        .files()
        .to_vec();
    assert_eq!(take_key.len(), 1);
    let take_key = take_key[0].clone();
    assert_eq!(take_key.parent().and_then(|p| p.to_str()), Some("audio"));
    assert!(take_key
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("vox ")));
    println!(
        "recording into {:?}",
        session.recording.as_ref().map(|r| r.files().to_vec())
    );

    // 48_000 / 2 / 256 = 93 blocks, 23_808 samples, 0.496 s of a 440 Hz
    // sine at amplitude 0.4. The same block is pushed to the mic and to
    // the monitor. Halfway is `n == 46`, the 47th block: 12_032 frames
    // captured, none sealed (the file flushes every 48_000 frames), nothing
    // dropped, no disk errors, file peak 0.40. The strip uses the centre
    // constant-power pan, so the monitor peaks at 0.283.
    let mut phase = 0.0f32;
    let mut monitored = 0.0f32;
    let mut sung = PlanarBuffer::new(1, block);
    let blocks = rate.hz() as usize / 2 / block;
    assert_eq!(blocks, 93);
    let centre = PanLaw::ConstantPower.gains(0.0).0;
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
            assert_eq!(transport::state(&session), TransportState::Recording);
            assert_eq!(frames, 12_032);
            assert_eq!(r.sealed_frames(), 0);
            assert_eq!(r.dropped(), 0);
            assert_eq!(r.disk_errors(), 0);
            assert!((r.peaks()[0] - 0.4).abs() < 1e-3);
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
    assert!((monitored - 0.4 * centre).abs() < 1e-3);

    println!("heard while recording: peak {monitored:.3} on the vox strip");

    // Stop finishes the file: 23_808 frames of 32-bit float, one channel,
    // plus an 80-byte header, 95_312 bytes. The clip runs from sample
    // 47_872 to sample 71_680 (0.997 s .. 1.493 s), offset 0 into the file.
    // The undo label is "Record".
    let clips = record::stop(&mut session)?;
    let wav = folder.join(&take_key);
    let bytes = std::fs::metadata(&wav).map(|m| m.len()).unwrap_or(0);
    assert_eq!(bytes, 95_312);
    if let Ok(entries) = std::fs::read_dir(folder.join("audio")) {
        for entry in entries.flatten() {
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            println!("written {} ({bytes} bytes)", entry.path().display());
        }
    }
    assert_eq!(clips.len(), 1);
    for id in &clips {
        let clip = session.memory.clip(*id)?;
        assert_eq!(
            clip.name,
            take_key.file_stem().unwrap().to_string_lossy().as_ref()
        );
        assert_eq!(clip.start().samples(), 47_872);
        assert_eq!(clip.length().samples(), 23_808);
        assert_eq!((clip.start() + clip.length()).samples(), 71_680);
        match &clip.source {
            ClipSource::Audio { path, offset } => {
                assert_eq!(path, &take_key);
                assert_eq!(offset.samples(), 0);
            }
            _ => panic!("the take is an audio clip"),
        }
        println!(
            "clip {:?} on {}: {:.3} s .. {:.3} s (source {:?})",
            clip.name,
            session.memory.track(session.memory.clip_track(*id)?)?.name,
            clip.start().to_seconds(rate),
            (clip.start() + clip.length()).to_seconds(rate),
            clip.source
        );
    }
    assert_eq!(session.history.undo_label(), Some("Record"));
    println!("undo label: {:?}", session.history.undo_label());

    // From the punch-in, eight blocks of the clip. The file is the raw
    // 0.4 sine; the strip's centre pan brings the peak to 0.283.
    transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
    transport::start(&mut session)?;
    let heard = session::steps::render_blocks(&mut session, 8)?;
    assert!((heard.peak() - 0.4 * centre).abs() < 1e-3);
    println!("playback of the clip, peak {:.3}", heard.peak());

    // Undo removes the clip and the file. The buffer stays in memory, so
    // `audio.len()` is 1, and nothing unfinished is left on disk.
    session::undo(&mut session)?;
    assert_eq!(session.memory.clip_count(), 0);
    assert_eq!(session.audio.len(), 1);
    assert_eq!(record::unfinished(&session).len(), 0);
    assert!(!wav.is_file());
    println!(
        "after undo: {} clips, {} recording(s) in memory, {} unfinished on disk",
        session.memory.clip_count(),
        session.audio.len(),
        record::unfinished(&session).len()
    );

    // With `devices`, an empty input list prints a line and returns. When
    // an input exists, vox is armed again and half a second is taken from
    // the default input (`Source::Device(None)`).
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
