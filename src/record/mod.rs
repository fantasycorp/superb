//! Recording: arm tracks, capture input, get a clip on each of them.
//!
//! [`start`] records from a [`Source`] onto every armed track, from the
//! playhead or from the punch-in point ahead of it ([`punch_in`]), until
//! [`stop`] or the punch-out point ([`punch_out`]); the transport rolls
//! from where it is and drops into and out of record at those points.
//! Every armed audio track's audio streams into its own WAV file in the
//! session folder (`audio/vox 1.wav`) from the first block, flushed about
//! once a second, so a crash costs a second and never the recording; [`stop`]
//! finishes the files, keeps the audio in memory, and places a clip on each
//! of those tracks, as one undo step; [`recover`] places what a crash left. An armed MIDI track gets a clip of the
//! notes that reached the engine meanwhile, from any port or
//! `midi::play_note`, stamped where they arrived. The audio arrives through a capture
//! queue that holds only a fraction of a second, so something has to empty
//! it as it fills: a hardware source gets a thread that does so; a queue
//! you fill yourself waits for [`collect`], which
//! [`session::steps::render_blocks`] calls after every block it renders.
//!
//! A device recording lands where it was played: its first frame is lined up
//! with where the transport began rolling, comparing the input's capture
//! instants with the output's playback instants on the device clock, so
//! the buffers in and out are taken back by themselves.
//!
//! Made of `superb::primitives::io::capture` (the queue),
//! `superb::primitives::io::wav::Writer` (the files), the session's audio
//! input (`devices::input`, feature `devices`), the audio held on the
//! `Session`, and [`clip::add`](crate::clip::add).
//!
//! ```
//! use superb::primitives::buffers::PlanarBuffer;
//! use superb::primitives::io::capture::capture_queue;
//! use superb::primitives::session::{ClipSource, TrackKind};
//! use superb::primitives::SampleRate;
//! use superb::{record, session, track};
//!
//! let folder = std::env::temp_dir().join(format!("superb-record-example-{}", std::process::id()));
//! let mut session = session::create(&folder, SampleRate::HZ_48K, 64)?;
//! let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
//! track::arm(&mut session, vox, true)?;
//! assert!(session.memory.track(vox).unwrap().state.arm);
//!
//! let (mut mic, blocks) = capture_queue(1, 64, 16)?;
//! record::start(
//!     &mut session,
//!     record::Source::Queue {
//!         blocks,
//!         channels: 1,
//!     },
//! )?;
//! let mut block = PlanarBuffer::new(1, 64);
//! block.fill(0.25);
//! for i in 0..5u64 {
//!     assert!(mic.push(&block, i));
//! }
//! let clips = record::stop(&mut session)?;
//! let file = folder.join("audio/vox 1.wav");
//! assert!(file.is_file());
//! let (read, rate) = superb::primitives::io::wav::read(&file)?;
//! assert_eq!(rate, SampleRate::HZ_48K);
//! assert_eq!(read.frames(), 320);
//! assert_eq!(read.channels(), 1);
//! assert!(read.channel(0).iter().all(|&s| s == 0.25));
//! let clip = session.memory.clip(clips[0]).unwrap();
//! assert_eq!(clip.name, "vox 1");
//! assert!(matches!(
//!     &clip.source,
//!     ClipSource::Audio { path, .. } if path == std::path::Path::new("audio/vox 1.wav")
//! ));
//! assert!(session::undo(&mut session)?);
//! assert!(!file.exists());
//! let _ = std::fs::remove_dir_all(&folder);
//! # Ok::<(), superb::primitives::SuperbError>(())
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::primitives::buffers::PlanarBuffer;
use crate::primitives::io::capture::CaptureConsumer;
use crate::primitives::io::wav;
use crate::primitives::io::{CapturedMidi, EngineCommand, RollStart};
use crate::primitives::midi::MidiMessage;
use crate::primitives::session::commands::{AddClip, PunchPoint, SetPunch};
use crate::primitives::session::{Clip, ClipSource, Input, Memory, MidiNote, TrackKind};
use crate::primitives::transport::TempoMap;
use crate::primitives::undo::Command;
use crate::primitives::{
    BeatPosition, ClipId, RawId, Result, SampleRate, SuperbError, TimeDuration, TimePosition,
    TrackId,
};
use crate::session;
use crate::session::Session;
use crate::transport;

mod capture;
mod files;
mod recovery;
pub(crate) mod steps;
#[cfg(test)]
mod tests;

pub use capture::Recording;
pub(crate) use capture::{Capture, Collector, Health};
pub(crate) use files::{Entry, Files};
use files::{RecordClip, Tag, TAG_FORMAT_VERSION, TAG_SUFFIX};
pub use recovery::{recover, unfinished};

/// Where a recording's audio comes from.
pub enum Source {
    /// The session's audio input (`devices::input`). If it is not open
    /// yet, this opens it on the named device (`None` for the default).
    /// Every channel the input carries is captured, and a thread gathers
    /// the audio until [`stop`]; each track keeps the channel(s) its
    /// [`track::input`](crate::track::input) names.
    #[cfg(feature = "devices")]
    Device(Option<String>),
    /// A capture queue you fill yourself (a file, a generator, a test) with
    /// blocks of `channels` channels. Blocks wait on the queue until
    /// [`collect`] runs; [`session::steps::render_blocks`] does that after
    /// every block, so an offline program never calls it.
    Queue {
        /// The queue's consumer end (`capture_queue` gives you both ends).
        blocks: CaptureConsumer,
        /// Channels in every block.
        channels: usize,
    },
}

/// Starts recording from `source` onto every armed track. Recording begins
/// at the playhead, or at the punch-in point when one is set ahead of it
/// ([`punch_in`]): the transport rolls from where it is and drops into
/// record there. With a punch-out point ([`punch_out`]) it drops out of
/// record there and keeps playing; [`stop`] finishes either way. A
/// recording that starts at the playhead gets the metronome's count-in
/// (`metronome::count_in`): the transport rolls that many bars early with
/// the click sounding. Every armed audio track gets its file in the
/// session folder now (`audio/vox 1.wav`, with a `.recording` tag beside
/// it) and the audio streams into it from the first block.
///
/// Made of: [`track::arm`](crate::track::arm)ed tracks → for a device,
/// `devices::input` if needed and its recording queue → a `wav::Writer`
/// and a tag per armed audio track → a collector thread for a device →
/// [`session::steps::rebuild`] when needed → `EngineHandle::send(SetPunch)`
/// → `EngineHandle::send(Record)`, or `Play` when the punch-in is ahead →
/// [`transport::playhead`] for where it all started.
pub fn start(session: &mut Session, source: Source) -> Result<()> {
    if session.recording.is_some() {
        return Err(SuperbError::InvalidState {
            why: "a recording is already running",
        });
    }
    if crate::track::armed(session).is_empty() {
        return Err(SuperbError::InvalidState {
            why: "arm a track before recording",
        });
    }
    let audio = crate::track::armed(session).iter().any(|t| {
        session
            .memory
            .track(*t)
            .is_ok_and(|t| t.kind == TrackKind::Audio)
    });
    if !audio {
        // MIDI only: nothing to capture from the audio side.
        return begin(session, None);
    }
    match source {
        #[cfg(feature = "devices")]
        Source::Device(device) => {
            if session.input.is_none() {
                crate::devices::input(session, device.as_deref())?;
            }
            let input = session.input.as_mut().ok_or(SuperbError::InvalidState {
                why: "the audio input did not open",
            })?;
            let channels = input.channels();
            let blocks = input.begin_recording()?;
            begin(session, Some((blocks, channels, true)))
        }
        Source::Queue { blocks, channels } => begin(session, Some((blocks, channels, false))),
    }
}

/// What every source has in common once the queue exists: `(the queue's
/// consumer end, its channels, whether a thread should drain it)`.
fn begin(session: &mut Session, capture: Option<(CaptureConsumer, usize, bool)>) -> Result<()> {
    let tracks = crate::track::armed(session);
    if session.needs_rebuild {
        session::steps::rebuild(session)?;
    }
    let playhead = transport::playhead(session);
    let punch_in = session
        .memory
        .punch_in()
        .filter(|p| *p > playhead)
        .unwrap_or(playhead);
    let punch_out = session.memory.punch_out().filter(|p| *p > punch_in);
    let (started_at, count_in) = roll_from(session, punch_in, playhead);
    let rolls_before = session.engine.roll_start().map_or(0, |r| r.rolls);
    let channels = capture.as_ref().map_or(0, |c| c.1);
    let health = Health::new(channels);
    let (capture, keys) = match capture {
        None => (None, Vec::new()),
        Some((blocks, channels, threaded)) => {
            let files = steps::open_files(
                session,
                &tracks,
                channels,
                started_at,
                punch_in,
                punch_out,
                Arc::clone(&health),
            )?;
            let keys = files.entries.iter().map(|e| e.key.clone()).collect();
            let capture = if threaded {
                Capture::Threaded(steps::spawn_collector(blocks, files)?)
            } else {
                Capture::Manual { blocks, files }
            };
            (Some(capture), keys)
        }
    };
    if started_at != playhead {
        session.engine.send(EngineCommand::Seek(started_at))?;
    }
    // Incoming MIDI from here on belongs to this recording.
    while session.engine.captured_midi().is_some() {}
    session.engine.send(EngineCommand::CaptureMidi(true))?;
    session.engine.send(EngineCommand::SetPunch {
        punch_in: (started_at < punch_in).then_some(punch_in),
        punch_out,
    })?;
    session.engine.send(if started_at < punch_in {
        EngineCommand::Play
    } else {
        EngineCommand::Record
    })?;
    session.recording = Some(Recording {
        capture,
        midi: Vec::new(),
        started_at,
        punch_in,
        punch_out,
        tracks,
        channels,
        rolls_before,
        keys,
        health,
    });
    if count_in {
        session.mixer.click.set_count_in_until(Some(punch_in));
    }
    Ok(())
}

/// Where the transport starts rolling for a recording that begins at
/// `punch_in`, and whether that is a count-in: the metronome's bars of
/// count-in before the playhead when the recording starts there, otherwise
/// the playhead itself (or the punch-in, if the playhead is past it).
fn roll_from(
    session: &Session,
    punch_in: TimePosition,
    playhead: TimePosition,
) -> (TimePosition, bool) {
    let bars = session.memory.metronome().count_in;
    if bars == 0 || punch_in != playhead {
        return (playhead.min(punch_in), false);
    }
    let tempo = session.memory.tempo_map();
    let beat = tempo.samples_to_beat(punch_in);
    let per_bar = f64::from(tempo.meter_at(beat).numerator());
    let from = BeatPosition::new((beat.beats() - f64::from(bars) * per_bar).max(0.0));
    let from = tempo.beat_to_samples(from);
    (from, from < punch_in)
}

/// Sets or clears the punch-in point: where the next recording begins when
/// it lies ahead of the playhead. Saved with the session; an edit that
/// coalesces like a drag.
///
/// Made of: `SetPunch` through `session::edit`.
pub fn punch_in(session: &mut Session, at: Option<TimePosition>) -> Result<()> {
    session::edit(session, SetPunch::new(PunchPoint::In, at))
}

/// Sets or clears the punch-out point: where a recording drops out of
/// record on its own and keeps playing. Saved with the session; an edit
/// that coalesces like a drag.
///
/// Made of: `SetPunch` through `session::edit`.
pub fn punch_out(session: &mut Session, at: Option<TimePosition>) -> Result<()> {
    session::edit(session, SetPunch::new(PunchPoint::Out, at))
}

/// Writes waiting audio blocks to the recording's files when the source is
/// a queue ([`Source::Queue`]), and takes the MIDI that arrived since the
/// last call either way; a device recording's audio has its own thread.
/// Returns the frames captured in total, or zero when nothing is recording.
/// [`session::steps::render_blocks`] calls this after every block; with a
/// device driving the engine, call it from a timer during long recordings, so
/// the MIDI queue (`MIDI_CAPTURE_CAPACITY` messages) never fills.
pub fn collect(session: &mut Session) -> usize {
    let Some(recording) = session.recording.as_mut() else {
        return 0;
    };
    recording.collect();
    recording.gather_midi(&mut session.engine);
    recording.frames()
}

/// Stops the recording and the transport, finishes the files and places one
/// clip per armed track. An audio clip refers to its track's file
/// (`audio/vox 1.wav`, so it is named `vox 1`) from the punch-in point:
/// what was captured before it (the roll towards it, a count-in) stays in
/// the file ahead of the clip's offset, and what came after the punch-out
/// stays past its end, so the clip can be trimmed out over it later like
/// any other. The file holds the channels the track's [`Input`] selects
/// (tracks without one get the whole capture). A device recording lands where
/// it was played: the file's first frame is placed by the device clock,
/// and a capture that began after the transport rolled gives a clip that
/// starts that much later. Returns the new clips (none if nothing was
/// captured). The whole placement is one undo step called "Record":
/// undoing it removes the clips and deletes the files it wrote, redo
/// writes them again from the audio kept in memory.
///
/// Made of: `EngineHandle::send(Stop)` → drain the queue or
/// `Collector::finish` (and hand the audio input its queue back) →
/// `wav::Writer::finish` per file, tags removed → the file's first frame
/// lined up with the `RollStart` when the device clock gives one → per
/// track, the clip's start, offset into the file and length from the punch
/// points → `wav::read` into `AudioCache::insert_shared` →
/// [`clip::add`](crate::clip::add), inside one undo group; rebuilds the
/// mixer so the clips play back on their strips.
pub fn stop(session: &mut Session) -> Result<Vec<ClipId>> {
    let recording = session.recording.take().ok_or(SuperbError::InvalidState {
        why: "nothing is being recorded",
    })?;
    session.mixer.click.set_count_in_until(None);
    session.engine.send(EngineCommand::CaptureMidi(false))?;
    let ended_at = session.engine.position();
    session.engine.send(EngineCommand::SetPunch {
        punch_in: None,
        punch_out: None,
    })?;
    session.engine.send(EngineCommand::Stop)?;
    let Recording {
        capture,
        mut midi,
        started_at,
        punch_in,
        punch_out,
        tracks,
        rolls_before,
        ..
    } = recording;
    while let Some(message) = session.engine.captured_midi() {
        midi.push(message);
    }
    let rate = session.memory.sample_rate();
    // The audio side, when an audio track was armed.
    let mut finished = Vec::new();
    let mut origin = started_at.samples();
    if let Some(capture) = capture {
        let files = match capture {
            Capture::Manual {
                mut blocks,
                mut files,
            } => {
                files.append(&mut blocks);
                files
            }
            Capture::Threaded(collector) => {
                let (files, blocks) = collector.finish()?;
                #[cfg(feature = "devices")]
                if let Some(input) = session.input.as_mut() {
                    input.end_recording(blocks);
                }
                #[cfg(not(feature = "devices"))]
                drop(blocks);
                files
            }
        };
        // Frame 0 of the files is where the transport began rolling, when
        // the device clock can say so.
        let lead = steps::capture_lead(
            files.first_us(),
            session.engine.roll_start(),
            rolls_before,
            started_at,
            rate,
        );
        origin -= lead;
        finished = files.finish()?;
    }
    session.history.begin_group("Record");
    let mut clips = Vec::with_capacity(tracks.len());
    for track in &tracks {
        let kind = session
            .memory
            .track(*track)
            .map(|t| t.kind)
            .unwrap_or(TrackKind::Audio);
        if kind == TrackKind::Midi {
            let until = punch_out.unwrap_or(ended_at);
            let Some(clip) = steps::midi_clip(session, *track, &midi, punch_in, until) else {
                continue;
            };
            let id = clip.id();
            if let Err(e) = session::structural(session, AddClip::new(*track, clip)) {
                session.history.cancel_group(&mut session.memory)?;
                return Err(e);
            }
            clips.push(id);
            continue;
        }
        let Some(file) = finished.iter().find(|f| f.track == *track) else {
            continue;
        };
        let Some((start, offset, length)) =
            steps::placement(origin, file.frames, punch_in, punch_out)
        else {
            // Nothing of it falls inside the recording: an empty file nobody refers to.
            let _ = std::fs::remove_file(&file.path);
            continue;
        };
        let placed = wav::read(&file.path)
            .map(|(audio, _)| Arc::new(audio))
            .and_then(|audio| {
                session
                    .audio
                    .insert_shared(file.key.clone(), Arc::clone(&audio));
                let id = session.memory.allocate_clip_id();
                let name = file
                    .key
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let clip = Clip::with_id(
                    id,
                    name,
                    start,
                    length,
                    ClipSource::Audio {
                        path: file.key.clone(),
                        offset,
                    },
                )?;
                let folder = session.folder.clone();
                session::structural(
                    session,
                    RecordClip {
                        add: AddClip::new(*track, clip),
                        folder,
                        key: file.key.clone(),
                        audio,
                        rate,
                    },
                )
                .map(|()| id)
            });
        match placed {
            Ok(id) => clips.push(id),
            Err(e) => {
                session.history.cancel_group(&mut session.memory)?;
                return Err(e);
            }
        }
    }
    session.history.end_group();
    session.needs_rebuild = true;
    Ok(clips)
}
