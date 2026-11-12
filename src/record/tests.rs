//! Tests of the record flow.

use super::*;
use crate::primitives::io::capture::capture_queue;
use crate::primitives::{SampleRate, TimeDuration};
use crate::track;
use crate::transport;

const R: SampleRate = SampleRate::HZ_48K;

/// A "microphone": a capture queue we fill by hand with a constant value.
fn feed(producer: &mut crate::primitives::io::capture::CaptureProducer, blocks: usize, value: f32) {
    let mut block = PlanarBuffer::new(1, 64);
    block.fill(value);
    for i in 0..blocks {
        assert!(producer.push(&block, i as u64 * 1_000), "queue has room");
    }
}

#[test]
fn a_recording_becomes_a_clip_on_every_armed_track() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    let gtr = track::add(&mut session, "gtr", TrackKind::Audio).unwrap();
    let _idle = track::add(&mut session, "idle", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    track::arm(&mut session, gtr, true).unwrap();
    assert_eq!(crate::track::armed(&session), [vox, gtr]);
    assert!(session.recording.is_none());

    // Roll to 640 frames, then punch in there.
    transport::start(&mut session).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap();
    let (mut mic, blocks) = capture_queue(1, 64, 16).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    assert!(session.recording.is_some());
    assert_eq!(
        session.recording.as_ref().unwrap().punch_in(),
        TimePosition::from_samples(640)
    );
    session::steps::render_blocks(&mut session, 5).unwrap();
    assert_eq!(
        transport::state(&session),
        crate::primitives::transport::TransportState::Recording
    );
    feed(&mut mic, 5, 0.25); // 320 frames of "singing"

    let clips = stop(&mut session).unwrap();
    assert_eq!(clips.len(), 2);
    assert!(session.recording.is_none());
    for (clip, name) in clips.iter().zip(["vox 1", "gtr 1"]) {
        let c = session.memory.clip(*clip).unwrap();
        assert_eq!(c.start(), TimePosition::from_samples(640));
        assert_eq!(c.length(), TimeDuration::from_samples(320));
        assert_eq!(c.name, name);
    }
    assert_eq!(session.memory.clip_track(clips[0]).unwrap(), vox);
    assert_eq!(session.memory.clip_track(clips[1]).unwrap(), gtr);
    assert_eq!(session.audio.len(), 2, "one entry per clip");
    assert_eq!(
        session
            .audio
            .get("audio/vox 1.wav".as_ref())
            .unwrap()
            .frames(),
        320,
        "each track has its own file"
    );
    assert!(matches!(
        &session.memory.clip(clips[0]).unwrap().source,
        ClipSource::Audio { offset, .. } if offset.samples() == 0
    ));
    assert_eq!(session.history.undo_label(), Some("Record"));

    // Stopping stopped the transport; the clips play back from their place.
    session::steps::render_blocks(&mut session, 1).unwrap();
    assert_eq!(
        transport::state(&session),
        crate::primitives::transport::TransportState::Stopped
    );
    transport::seek(&mut session, TimePosition::from_samples(640)).unwrap();
    transport::start(&mut session).unwrap();
    let heard = session::steps::render_blocks(&mut session, 4).unwrap();
    assert!(heard.channel(0)[200].abs() > 0.2, "two tracks of the clip");
    assert_eq!(
        session.mixer.strips.len(),
        4,
        "one handle per track and the master"
    );

    // One undo removes both clips.
    assert!(session::undo(&mut session).unwrap());
    assert!(session.memory.clip(clips[0]).is_err() && session.memory.clip(clips[1]).is_err());
    assert!(session::redo(&mut session).unwrap());
    assert!(session.memory.clip(clips[0]).is_ok());
}

#[test]
fn clips_get_distinct_names_and_empty_recordings_leave_no_clip() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    for i in 1..=2 {
        let (mut mic, blocks) = capture_queue(2, 64, 4).unwrap();
        start(
            &mut session,
            Source::Queue {
                blocks,
                channels: 2,
            },
        )
        .unwrap();
        feed_stereo(&mut mic, 2);
        let clips = stop(&mut session).unwrap();
        assert_eq!(
            session.memory.clip(clips[0]).unwrap().name,
            format!("vox {i}")
        );
    }
    assert_eq!(session.audio.len(), 2);
    let (_mic, blocks) = capture_queue(2, 64, 4).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 2,
        },
    )
    .unwrap();
    assert!(
        stop(&mut session).unwrap().is_empty(),
        "nothing captured, nothing placed"
    );
    assert_eq!(session.memory.clip_count(), 2);
}

fn feed_stereo(producer: &mut crate::primitives::io::capture::CaptureProducer, blocks: usize) {
    let block = PlanarBuffer::new(2, 64);
    for i in 0..blocks {
        assert!(producer.push(&block, i as u64));
    }
}

#[test]
fn tracks_record_the_input_channels_they_chose() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    let gtr = track::add(&mut session, "gtr", TrackKind::Audio).unwrap();
    let room = track::add(&mut session, "room", TrackKind::Audio).unwrap();
    for t in [vox, gtr, room] {
        track::arm(&mut session, t, true).unwrap();
    }
    track::input(&mut session, vox, Some(Input::Mono(0))).unwrap();
    track::input(&mut session, gtr, Some(Input::Mono(1))).unwrap();
    track::input(&mut session, room, Some(Input::Stereo(0, 1))).unwrap();
    assert_eq!(steps::input_width(&session), 2);
    let keys = track::add(&mut session, "keys", TrackKind::Midi).unwrap();
    assert!(track::input(&mut session, keys, Some(Input::Mono(0))).is_err());

    // A two-channel capture: 0.25 on the left, 0.5 on the right.
    let (mut mic, blocks) = capture_queue(2, 64, 8).unwrap();
    let mut block = PlanarBuffer::new(2, 64);
    block.channel_mut(0).fill(0.25);
    block.channel_mut(1).fill(0.5);
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 2,
        },
    )
    .unwrap();
    for i in 0..3 {
        assert!(mic.push(&block, i));
    }
    let clips = stop(&mut session).unwrap();
    assert_eq!(clips.len(), 3);
    {
        let audio = |name: &str| {
            session
                .audio
                .get(format!("audio/{name}.wav").as_ref())
                .unwrap()
        };
        assert_eq!(audio("vox 1").channels(), 1);
        assert!(audio("vox 1").channel(0).iter().all(|&s| s == 0.25));
        assert_eq!(audio("gtr 1").channels(), 1);
        assert!(audio("gtr 1").channel(0).iter().all(|&s| s == 0.5));
        assert_eq!(audio("room 1").channels(), 2);
        assert!(audio("room 1").channel(1).iter().all(|&s| s == 0.5));
        assert_eq!(audio("room 1").frames(), 192);
    }
    // A channel the capture did not have is silence.
    let far = track::add(&mut session, "far", TrackKind::Audio).unwrap();
    track::arm(&mut session, far, true).unwrap();
    track::input(&mut session, far, Some(Input::Mono(5))).unwrap();
    let (mut mic, blocks) = capture_queue(2, 64, 8).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 2,
        },
    )
    .unwrap();
    assert!(mic.push(&block, 0));
    let clips = stop(&mut session).unwrap();
    assert_eq!(clips.len(), 4);
    let silent = session.audio.get("audio/far 1.wav".as_ref()).unwrap();
    assert!(silent.is_silent() && silent.frames() == 64 && silent.channels() == 1);
    // No input set: the whole capture.
    track::input(&mut session, vox, None).unwrap();
    assert_eq!(steps::input_width(&session), 6, "far still needs six");
}

#[test]
fn errors_are_explicit() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let bus = track::add(&mut session, "bus", TrackKind::Bus).unwrap();
    assert!(
        track::arm(&mut session, bus, true).is_err(),
        "buses are not armed"
    );
    assert!(
        matches!(stop(&mut session), Err(SuperbError::InvalidState { .. })),
        "nothing recording"
    );
    let (_mic, blocks) = capture_queue(1, 64, 2).unwrap();
    assert!(
        start(
            &mut session,
            Source::Queue {
                blocks,
                channels: 1
            }
        )
        .is_err(),
        "no armed track"
    );
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    let (_a, blocks) = capture_queue(1, 64, 2).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    let (_b, blocks) = capture_queue(1, 64, 2).unwrap();
    assert!(
        start(
            &mut session,
            Source::Queue {
                blocks,
                channels: 1
            }
        )
        .is_err(),
        "already recording"
    );
    let _ = format!("{:?}", session.recording.as_ref().unwrap());
}

#[test]
fn a_collector_thread_gathers_the_take_by_itself() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    // A queue of 64 blocks fed with 200, paced like a device, and no
    // `collect` calls: the thread keeps up on its own. (A failed push
    // counts as a drop, so the accounting below must balance exactly.)
    let (mut mic, blocks) = capture_queue(1, 64, 64).unwrap();
    begin(&mut session, Some((blocks, 1, true))).unwrap();
    assert!(matches!(
        session.recording.as_ref().unwrap().capture,
        Some(Capture::Threaded(_))
    ));
    let mut block = PlanarBuffer::new(1, 64);
    block.fill(0.5);
    for i in 0..200u64 {
        mic.push(&block, i);
        std::thread::sleep(core::time::Duration::from_micros(300));
    }
    // `collect` has nothing to do for a threaded capture; it reports.
    let _ = collect(&mut session);
    let _ = format!("{:?}", session.recording.as_ref().unwrap());
    let clips = stop(&mut session).unwrap();
    let dropped = mic.dropped() as usize;
    let clip = session.memory.clip(clips[0]).unwrap();
    assert_eq!(
        clip.length(),
        TimeDuration::from_samples(((200 - dropped) * 64) as i64),
        "every block pushed was gathered or counted as dropped"
    );
    assert!(
        dropped < 100,
        "the collector fell far behind: {dropped} dropped"
    );
    let audio = session.audio.get("audio/vox 1.wav".as_ref()).unwrap();
    assert!(audio.channel(0).iter().all(|&s| s == 0.5));

    // An unfinished collector just lets its thread go; its file and
    // tag stay for `recover`.
    let (_mic, blocks) = capture_queue(1, 64, 2).unwrap();
    begin(&mut session, Some((blocks, 1, true))).unwrap();
    let keys = session.recording.as_ref().unwrap().files().to_vec();
    assert_eq!(keys, [PathBuf::from("audio/vox 2.wav")]);
    session.recording = None;
    assert!(session.folder.join("audio/vox 2.wav.recording").is_file());
    assert_eq!(unfinished(&session), keys);
}

#[test]
fn files_pad_missing_channels_and_flush_on_a_cadence() {
    let mut session = session::create(session::scratch("files"), R, 4).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    let health = Health::new(2);
    // A capture said to have two channels; the blocks bring one.
    let mut files = steps::open_files(
        &session,
        &[vox],
        2,
        TimePosition::ZERO,
        TimePosition::ZERO,
        None,
        Arc::clone(&health),
    )
    .unwrap();
    files.flush_every = 8;
    let (mut mic, mut blocks) = capture_queue(1, 4, 4).unwrap();
    let mut block = PlanarBuffer::new(1, 4);
    block.fill(1.0);
    mic.push(&block, 7);
    assert_eq!(files.append(&mut blocks), 4);
    assert_eq!(files.frames(), 4);
    assert_eq!(files.first_us(), Some(7));
    assert_eq!(health.sealed.load(Ordering::Acquire), 0, "not flushed yet");
    mic.push(&block, 8);
    assert_eq!(files.append(&mut blocks), 4);
    assert_eq!(
        health.sealed.load(Ordering::Acquire),
        8,
        "flushed at 8 frames"
    );
    assert_eq!(files.append(&mut blocks), 0);
    assert!(blocks.is_empty());
    let tag = session.folder.join("audio/vox 1.wav.recording");
    assert!(tag.is_file());
    let finished = files.finish().unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].frames, 8);
    assert!(!tag.exists(), "finished: the tag is gone");
    let (audio, _) = wav::read(&finished[0].path).unwrap();
    assert_eq!(audio.channels(), 2);
    assert_eq!(audio.channel(0), &[1.0; 8]);
    assert_eq!(audio.channel(1), &[0.0; 8]);
}

#[test]
fn collecting_as_you_go_outlasts_the_queue() {
    let mut session = session::create(session::scratch("rec"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    // A queue of only two blocks; the recording is ten blocks long.
    let (mut mic, blocks) = capture_queue(1, 64, 2).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    for i in 0..10 {
        feed(&mut mic, 1, 0.5);
        // Advancing the engine collects for us (a UI timer would call
        // `collect` when a device drives the engine).
        session::steps::render_blocks(&mut session, 1).unwrap();
        assert_eq!(session.recording.as_ref().unwrap().frames(), (i + 1) * 64);
    }
    assert_eq!(session.recording.as_ref().unwrap().dropped(), 0);
    assert_eq!(collect(&mut session), 640);
    let clips = stop(&mut session).unwrap();
    assert_eq!(
        session.memory.clip(clips[0]).unwrap().length(),
        TimeDuration::from_samples(640)
    );
    assert_eq!(collect(&mut session), 0, "nothing recording");

    // Without collecting, the input side reports what it had to drop.
    let (mut mic, blocks) = capture_queue(1, 64, 2).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    feed_lossy(&mut mic, 5, 0.5);
    assert_eq!(session.recording.as_ref().unwrap().dropped(), 3);
    stop(&mut session).unwrap();
}

fn feed_lossy(
    producer: &mut crate::primitives::io::capture::CaptureProducer,
    blocks: usize,
    value: f32,
) {
    let mut block = PlanarBuffer::new(1, 64);
    block.fill(value);
    for i in 0..blocks {
        let _ = producer.push(&block, i as u64);
    }
}

#[test]
fn a_recording_is_written_into_the_session_folder_as_it_stops() {
    let dir = session::scratch("rec-folder");
    let mut session = session::create(&dir, R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();

    let (mut mic, blocks) = capture_queue(1, 64, 16).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    feed(&mut mic, 5, 0.25);
    let clips = stop(&mut session).unwrap();
    let file = dir.join("audio/vox 1.wav");
    assert!(file.is_file(), "written as it stopped, before any save");
    let (read, rate) = crate::primitives::io::wav::read(&file).unwrap();
    assert_eq!(rate, R);
    assert_eq!(read.frames(), 320);
    assert_eq!(read.channels(), 1);
    assert!(read.channel(0).iter().all(|&s| s == 0.25));
    let clip = session.memory.clip(clips[0]).unwrap();
    assert_eq!(clip.name, "vox 1");
    assert!(matches!(
        &clip.source,
        crate::primitives::session::ClipSource::Audio { path, .. }
            if path == std::path::Path::new("audio/vox 1.wav")
    ));

    // Undoing the recording deletes its file; redo writes it again. After
    // an undo nothing refers to the name, so a reopened session reuses it.
    assert!(session::undo(&mut session).unwrap());
    assert!(!file.exists(), "undo deletes the recording's file");
    assert!(session::redo(&mut session).unwrap());
    assert!(file.is_file(), "redo writes it again");
    assert!(session::undo(&mut session).unwrap());
    assert!(!file.exists());
    session::save(&mut session).unwrap();
    let mut again = session::open(&dir, 64).unwrap();
    assert!(
        again.audio.get("audio/vox 1.wav".as_ref()).is_none(),
        "unreferenced"
    );
    track::arm(&mut again, vox, true).unwrap();
    let (mut mic, blocks) = capture_queue(1, 64, 16).unwrap();
    start(
        &mut again,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    feed(&mut mic, 2, 0.5);
    let clips = stop(&mut again).unwrap();
    assert_eq!(again.memory.clip(clips[0]).unwrap().name, "vox 1");
    assert!(file.is_file());

    // Reopening loads the recording and it plays.
    session::save(&mut again).unwrap();
    let mut played = session::open(&dir, 64).unwrap();
    assert_eq!(played.audio.len(), 1);
    transport::seek(&mut played, TimePosition::ZERO).unwrap();
    transport::start(&mut played).unwrap();
    assert!(
        session::steps::render_blocks(&mut played, 4)
            .unwrap()
            .peak()
            > 0.2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_crash_leaves_files_that_recover_places() {
    let dir = session::scratch("recover");
    let mut session = session::create(&dir, R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    let keys = track::add(&mut session, "keys", TrackKind::Midi).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    track::arm(&mut session, keys, true).unwrap();
    assert!(unfinished(&session).is_empty());
    assert!(
        recover(&mut session).unwrap().is_empty(),
        "nothing to recover"
    );

    // Roll to 640, record 10 blocks there, then "crash": the session
    // goes away without `stop`.
    transport::start(&mut session).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap();
    let (mut mic, blocks) = capture_queue(1, 64, 16).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    assert_eq!(
        session.recording.as_ref().unwrap().files(),
        [PathBuf::from("audio/vox 1.wav")]
    );
    for i in 0..10 {
        feed(&mut mic, 1, 0.25);
        session::steps::render_blocks(&mut session, 1).unwrap();
        let r = session.recording.as_ref().unwrap();
        assert_eq!(r.frames(), (i + 1) * 64);
        assert_eq!(r.peaks(), [0.25]);
        assert!(r.since_last_block().is_some());
        assert_eq!(r.disk_errors(), 0);
    }
    assert_eq!(
        session.recording.as_ref().unwrap().sealed_frames(),
        0,
        "a second has not passed in frames"
    );
    assert!(recover(&mut session).is_err(), "refused while recording");
    session::save(&mut session).unwrap();
    drop(session);
    let file = dir.join("audio/vox 1.wav");
    assert!(file.is_file() && dir.join("audio/vox 1.wav.recording").is_file());

    // Reopen: the recording is noted, not placed, until `recover`.
    let mut again = session::open(&dir, 64).unwrap();
    assert_eq!(again.memory.clip_count(), 0);
    assert_eq!(unfinished(&again), [PathBuf::from("audio/vox 1.wav")]);
    let clips = recover(&mut again).unwrap();
    assert_eq!(clips.len(), 1);
    let clip = again.memory.clip(clips[0]).unwrap();
    assert_eq!(clip.name, "vox 1");
    assert_eq!(clip.start(), TimePosition::from_samples(640));
    assert_eq!(clip.length(), TimeDuration::from_samples(640));
    assert_eq!(again.memory.clip_track(clips[0]).unwrap(), vox);
    assert!(unfinished(&again).is_empty(), "the tag is gone");
    assert!(!dir.join("audio/vox 1.wav.recording").exists());
    let (audio, _) = wav::read(&file).unwrap();
    assert_eq!(audio.frames(), 640, "repaired to what was written");
    assert!(audio.channel(0).iter().all(|&s| s == 0.25));
    assert_eq!(again.history.undo_label(), Some("Recover"));
    assert!(session::undo(&mut again).unwrap());
    assert!(file.is_file(), "undoing a recovery deletes nothing");
    assert!(recover(&mut again).unwrap().is_empty(), "no tags left");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn punch_points_decide_where_a_recording_begins_and_ends() {
    use crate::primitives::transport::TransportState;
    let mut session = session::create(session::scratch("punch"), R, 64).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    punch_in(&mut session, Some(TimePosition::from_samples(640))).unwrap();
    punch_out(&mut session, Some(TimePosition::from_samples(960))).unwrap();
    assert_eq!(
        session.memory.punch_in(),
        Some(TimePosition::from_samples(640))
    );
    let json = session.memory.to_json().unwrap();
    assert_eq!(
        crate::primitives::session::Memory::from_json(&json)
            .unwrap()
            .punch_out(),
        Some(TimePosition::from_samples(960))
    );

    // Start at zero: the transport rolls, drops into record at 640 and
    // out at 960; the clip holds exactly that span.
    let (mut mic, blocks) = capture_queue(1, 64, 64).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    let recording = session.recording.as_ref().unwrap();
    assert_eq!(recording.started_at(), TimePosition::ZERO);
    assert_eq!(recording.punch_in(), TimePosition::from_samples(640));
    assert_eq!(recording.punch_out(), Some(TimePosition::from_samples(960)));
    let mut seen_recording = false;
    for i in 0..20 {
        feed(&mut mic, 1, 0.25);
        session::steps::render_blocks(&mut session, 1).unwrap();
        let state = transport::state(&session);
        let position = transport::playhead(&session).samples();
        // The state switches as the block that reaches a point ends, so
        // the block starting at 640 already records and 960 does not.
        let expected = if (640..960).contains(&position) {
            TransportState::Recording
        } else {
            TransportState::Playing
        };
        assert_eq!(state, expected, "block {i} at {position}");
        seen_recording |= state == TransportState::Recording;
    }
    assert!(seen_recording);
    let clips = stop(&mut session).unwrap();
    let clip = session.memory.clip(clips[0]).unwrap();
    assert_eq!(clip.start(), TimePosition::from_samples(640));
    assert_eq!(clip.length(), TimeDuration::from_samples(320));
    let audio = session.audio.get("audio/vox 1.wav".as_ref()).unwrap();
    assert!(audio.channel(0).iter().all(|&s| s == 0.25));

    // A punch-in behind the playhead is ignored: recording starts now.
    transport::seek(&mut session, TimePosition::from_samples(2_000)).unwrap();
    session::steps::render_blocks(&mut session, 1).unwrap();
    punch_out(&mut session, None).unwrap();
    let (_mic, blocks) = capture_queue(1, 64, 4).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    assert_eq!(
        session.recording.as_ref().unwrap().punch_in(),
        TimePosition::from_samples(2_000)
    );
    session::steps::render_blocks(&mut session, 1).unwrap();
    assert_eq!(transport::state(&session), TransportState::Recording);
    stop(&mut session).unwrap();
    // The punch points are edits: undo clears the last one set.
    assert!(session::undo(&mut session).unwrap());
    assert_eq!(
        session.memory.punch_out(),
        Some(TimePosition::from_samples(960))
    );
}

#[test]
fn a_count_in_rolls_the_click_before_the_recording_begins() {
    use crate::primitives::transport::TransportState;
    let mut session = session::create(session::scratch("count-in"), R, 64).unwrap();
    // 1200 bpm in 4/4: a beat is 2 400 samples, a bar 9 600.
    session::tempo(&mut session, 1200.0).unwrap();
    crate::metronome::count_in(&mut session, 1).unwrap();
    let vox = track::add(&mut session, "vox", TrackKind::Audio).unwrap();
    track::arm(&mut session, vox, true).unwrap();
    transport::seek(&mut session, TimePosition::from_samples(9_600)).unwrap();
    session::steps::render_blocks(&mut session, 1).unwrap();

    let (mut mic, blocks) = capture_queue(1, 64, 64).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    let recording = session.recording.as_ref().unwrap();
    assert_eq!(recording.started_at(), TimePosition::ZERO, "one bar early");
    assert_eq!(recording.punch_in(), TimePosition::from_samples(9_600));
    assert_eq!(
        session.mixer.click.count_in_until(),
        Some(TimePosition::from_samples(9_600))
    );
    // The click sounds through the count-in although the metronome is off ...
    feed(&mut mic, 1, 0.25);
    let first = session::steps::render_blocks(&mut session, 1).unwrap();
    assert!(first.peak() > 0.3, "the downbeat tick: {}", first.peak());
    assert_eq!(transport::state(&session), TransportState::Playing);
    for _ in 1..160 {
        feed(&mut mic, 1, 0.25);
        session::steps::render_blocks(&mut session, 1).unwrap();
    }
    assert_eq!(transport::state(&session), TransportState::Recording);
    // ... and stops at the punch-in: well past it, nothing is heard.
    feed(&mut mic, 1, 0.25);
    assert!(session::steps::render_blocks(&mut session, 1)
        .unwrap()
        .is_silent());

    let clips = stop(&mut session).unwrap();
    assert!(session.mixer.click.count_in_until().is_none());
    let clip = session.memory.clip(clips[0]).unwrap();
    assert_eq!(clip.start(), TimePosition::from_samples(9_600));
    assert_eq!(clip.length(), TimeDuration::from_samples(161 * 64 - 9_600));
}

#[test]
fn a_capture_is_lined_up_with_the_roll_start_and_placed_by_offset() {
    use steps::{capture_lead, placement};
    let at = TimePosition::from_samples(1_000);
    let roll = RollStart {
        rolls: 3,
        position: at,
        instant_us: 1_010_000,
    };
    // No stamp, no roll start, or a roll start from an earlier
    // recording: nothing to line up.
    assert_eq!(capture_lead(None, Some(roll), 2, at, R), 0);
    assert_eq!(capture_lead(Some(1_000_000), None, 2, at, R), 0);
    assert_eq!(capture_lead(Some(1_000_000), Some(roll), 3, at, R), 0);
    // Capture began 10 ms before the roll: 480 frames too many.
    assert_eq!(capture_lead(Some(1_000_000), Some(roll), 2, at, R), 480);
    // Capture began 10 ms after the roll: 480 frames missing.
    assert_eq!(capture_lead(Some(1_020_000), Some(roll), 2, at, R), -480);
    // The recording begins 100 frames after where the transport rolled from.
    let later = TimePosition::from_samples(1_100);
    assert_eq!(capture_lead(Some(1_010_000), Some(roll), 2, later, R), 100);

    // Where the clip goes for a file whose frame 0 is at `origin`.
    let p = |origin: i64, frames: u64, punch_in: i64, out: Option<i64>| {
        placement(
            origin,
            frames,
            TimePosition::from_samples(punch_in),
            out.map(TimePosition::from_samples),
        )
        .map(|(s, o, l)| (s.samples(), o.samples(), l.samples()))
    };
    assert_eq!(p(0, 100, 0, None), Some((0, 0, 100)), "as captured");
    assert_eq!(
        p(0, 100, 40, None),
        Some((40, 40, 60)),
        "a roll towards the punch-in"
    );
    assert_eq!(
        p(0, 100, 40, Some(70)),
        Some((40, 40, 30)),
        "and out before the end"
    );
    assert_eq!(
        p(1_000, 100, 1_000, None),
        Some((1_000, 0, 100)),
        "elsewhere on the timeline"
    );
    assert_eq!(
        p(1_010, 100, 1_000, None),
        Some((1_010, 0, 100)),
        "capture began after the roll"
    );
    assert_eq!(
        p(990, 100, 1_000, None),
        Some((1_000, 10, 90)),
        "capture began before it"
    );
    assert_eq!(p(0, 100, 100, None), None, "nothing inside");
    assert_eq!(p(0, 0, 0, None), None, "nothing captured");
    assert_eq!(p(0, 100, 40, Some(40)), None, "zero length");
}

#[test]
fn a_midi_take_becomes_a_clip_on_the_armed_midi_track() {
    let mut session = session::create(session::scratch("midi-rec"), R, 64).unwrap();
    let keys = track::add(&mut session, "keys", TrackKind::Midi).unwrap();
    let bus = track::add(&mut session, "bus", TrackKind::Bus).unwrap();
    assert!(track::arm(&mut session, bus, true).is_err());
    track::arm(&mut session, keys, true).unwrap();
    transport::start(&mut session).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap(); // the playhead at 640
    let (_mic, blocks) = capture_queue(1, 64, 4).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    assert_eq!(
        session.recording.as_ref().unwrap().channels(),
        0,
        "no audio track armed, no audio captured"
    );
    crate::midi::play_note(&mut session, 0, 60, 100).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap();
    crate::midi::release_note(&mut session, 0, 60).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap();
    crate::midi::play_note(&mut session, 0, 64, 90).unwrap(); // left down
    session::steps::render_blocks(&mut session, 5).unwrap();
    assert_eq!(session.recording.as_ref().unwrap().midi_messages(), 3);

    let clips = stop(&mut session).unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(session.memory.clip_track(clips[0]).unwrap(), keys);
    let clip = session.memory.clip(clips[0]).unwrap();
    assert_eq!(clip.name, "keys");
    assert_eq!(clip.start(), TimePosition::from_samples(640));
    assert_eq!(clip.length(), TimeDuration::from_samples(1_600));
    let ClipSource::Midi { notes } = &clip.source else {
        panic!("a MIDI clip");
    };
    assert_eq!(notes.len(), 2);
    let beat = 1.0 / 24_000.0; // one sample, in beats, at 120 bpm and 48 kHz
    assert!(notes[0].beat.abs() < 1e-9);
    assert!((notes[0].length - 640.0 * beat).abs() < 1e-9);
    assert_eq!((notes[0].note, notes[0].velocity), (60, 100));
    assert!((notes[1].beat - 1_280.0 * beat).abs() < 1e-9);
    assert!(
        (notes[1].length - 320.0 * beat).abs() < 1e-9,
        "closed where the recording stopped"
    );
    assert_eq!(notes[1].note, 64);
    assert_eq!(session.history.undo_label(), Some("Record"));
    assert!(session::undo(&mut session).unwrap());
    assert!(session.memory.clip(clips[0]).is_err());
}

#[test]
fn each_midi_track_keeps_the_channel_it_listens_to() {
    let mut session = session::create(session::scratch("midi-ch"), R, 64).unwrap();
    let left = track::add(&mut session, "left", TrackKind::Midi).unwrap();
    let right = track::add(&mut session, "right", TrackKind::Midi).unwrap();
    let all = track::add(&mut session, "all", TrackKind::Midi).unwrap();
    track::midi_channel(&mut session, left, Some(0)).unwrap();
    track::midi_channel(&mut session, right, Some(1)).unwrap();
    for t in [left, right, all] {
        track::arm(&mut session, t, true).unwrap();
    }
    transport::start(&mut session).unwrap();
    let (_mic, blocks) = capture_queue(1, 64, 4).unwrap();
    start(
        &mut session,
        Source::Queue {
            blocks,
            channels: 1,
        },
    )
    .unwrap();
    crate::midi::play_note(&mut session, 0, 60, 100).unwrap();
    crate::midi::play_note(&mut session, 1, 64, 100).unwrap();
    session::steps::render_blocks(&mut session, 10).unwrap();
    crate::midi::release_note(&mut session, 0, 60).unwrap();
    crate::midi::release_note(&mut session, 1, 64).unwrap();
    session::steps::render_blocks(&mut session, 5).unwrap();
    let clips = stop(&mut session).unwrap();
    assert_eq!(clips.len(), 3);
    let pitches = |id: ClipId| match &session.memory.clip(id).unwrap().source {
        ClipSource::Midi { notes } => notes.iter().map(|n| n.note).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert_eq!(pitches(clips[0]), [60], "left: channel 0 only");
    assert_eq!(pitches(clips[1]), [64], "right: channel 1 only");
    assert_eq!(pitches(clips[2]), [60, 64], "all: both");
}
