//! The parts `start`, `collect`, `stop` and `recover` are made of.

use super::*;

/// Opens one file per armed audio track among `tracks`, each holding
/// the channels its [`Input`] selects from a capture of `channels`
/// channels (all of them without one), with a tag beside it. The file
/// is created at once, so the next track's name sees it taken.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_files(
    session: &Session,
    tracks: &[TrackId],
    channels: usize,
    started_at: TimePosition,
    punch_in: TimePosition,
    punch_out: Option<TimePosition>,
    health: Arc<Health>,
) -> Result<Files> {
    let rate = session.memory.sample_rate();
    let audio_dir = session.folder.join(crate::session::folder::AUDIO_DIR);
    std::fs::create_dir_all(&audio_dir).map_err(|e| SuperbError::io("create", &audio_dir, &e))?;
    let mut entries = Vec::new();
    for track in tracks {
        let Ok(t) = session.memory.track(*track) else {
            continue;
        };
        if t.kind != TrackKind::Audio {
            continue;
        }
        let pick: Vec<usize> = match t.state.input {
            None => (0..channels.max(1)).collect(),
            Some(Input::Mono(c)) => vec![c],
            Some(Input::Stereo(l, r)) => vec![l, r],
        };
        let key = audio_key(session, *track, &entries)?;
        let path = session.folder.join(&key);
        let writer = wav::Writer::create(&path, pick.len(), rate, wav::WavFormat::Float32)?;
        let tag = Tag {
            format_version: TAG_FORMAT_VERSION,
            track: track.raw(),
            key: key.clone(),
            started_at: started_at.samples(),
            punch_in: punch_in.samples(),
            punch_out: punch_out.map(|p| p.samples()),
            sample_rate: rate.hz(),
            channels: pick.len(),
            first_us: None,
        };
        tag.write(&path)?;
        entries.push(Entry {
            track: *track,
            key,
            path,
            pick,
            writer,
            tag,
        });
    }
    Ok(Files {
        entries,
        first_us: None,
        frames: 0,
        since_flush: 0,
        flush_every: rate.hz() as usize,
        silence: vec![0.0; session.engine.max_frames()],
        health,
    })
}

/// Starts a thread that writes `blocks` into `files` as they arrive.
/// The thread sleeps briefly when the queue is empty and exits after
/// [`Collector::finish`].
#[cfg_attr(not(feature = "devices"), allow(dead_code))]
pub(crate) fn spawn_collector(mut blocks: CaptureConsumer, mut files: Files) -> Result<Collector> {
    let dropped = blocks.dropped_handle();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = Arc::clone(&stop);
    let handle = std::thread::Builder::new()
        .name("superb recording collector".into())
        .spawn(move || loop {
            let appended = files.append(&mut blocks);
            if stop_t.load(Ordering::Acquire) {
                // The input has stopped feeding us; take what is left.
                files.append(&mut blocks);
                break (files, blocks);
            }
            if appended == 0 {
                std::thread::sleep(core::time::Duration::from_millis(2));
            }
        })
        .map_err(|e| SuperbError::Message(format!("cannot start the recording collector: {e}")))?;
    Ok(Collector {
        dropped,
        stop,
        handle: Some(handle),
    })
}

/// How many input channels the audio tracks' inputs name: one past the
/// highest channel among them, and at least one.
#[cfg_attr(not(feature = "devices"), allow(dead_code))]
pub(crate) fn input_width(session: &Session) -> usize {
    session
        .memory
        .tracks()
        .iter()
        .filter(|t| t.kind == TrackKind::Audio && !t.state.archived)
        .filter_map(|t| t.state.input)
        .map(Input::width)
        .max()
        .unwrap_or(1)
        .max(1)
}

/// Frames of the capture that came before the transport began rolling
/// (positive), or that are missing from its start (negative), from the
/// first block's device-clock stamp and the engine's [`RollStart`].
/// Zero when either side has no clock, or when the roll start is not
/// this recording's (`rolls` has not moved past `rolls_before`).
pub(crate) fn capture_lead(
    first_us: Option<u64>,
    roll: Option<RollStart>,
    rolls_before: u64,
    started_at: TimePosition,
    rate: SampleRate,
) -> i64 {
    let (Some(first_us), Some(roll)) = (first_us, roll) else {
        return 0;
    };
    if roll.rolls <= rolls_before {
        return 0;
    }
    let elapsed_us = i128::from(roll.instant_us) - i128::from(first_us);
    let to_roll = (elapsed_us * i128::from(rate.hz()) / 1_000_000) as i64;
    // Frame `to_roll` of the capture is the roll position; the recording
    // begins at `started_at`.
    to_roll + (started_at.samples() - roll.position.samples())
}

/// Where the clip for a file goes: `(start, offset into the file,
/// length)`, for a file of `frames` frames whose frame 0 is at
/// timeline sample `origin`, inside the punch points. The clip starts
/// at the punch-in, or at the file's first frame when the capture began
/// after it, and ends at the punch-out or the file's end. `None` when
/// nothing of the file falls inside.
pub(crate) fn placement(
    origin: i64,
    frames: u64,
    punch_in: TimePosition,
    punch_out: Option<TimePosition>,
) -> Option<(TimePosition, TimeDuration, TimeDuration)> {
    let file_end = origin + frames as i64;
    let start = punch_in.samples().max(origin);
    let end = punch_out.map_or(file_end, |out| out.samples().min(file_end));
    if end <= start {
        return None;
    }
    Some((
        TimePosition::from_samples(start),
        TimeDuration::from_samples(start - origin),
        TimeDuration::from_samples(end - start),
    ))
}

/// The tags in the session folder's `audio/`: `(tag file, tag)`, in
/// name order. A file that is not a tag is noted in the log and skipped.
pub(crate) fn tags(folder: &Path) -> Vec<(PathBuf, Tag)> {
    let dir = folder.join(crate::session::folder::AUDIO_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut tags: Vec<(PathBuf, Tag)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(TAG_SUFFIX))
        .filter_map(|p| match Tag::read(&p) {
            Ok(tag) => Some((p, tag)),
            Err(e) => {
                log::warn!("{e}");
                None
            }
        })
        .collect();
    tags.sort_by(|a, b| a.0.cmp(&b.0));
    tags
}

/// The notes among captured MIDI inside `[from, until)`, as a clip
/// starting at `from` would hold them: beats from `from`, a note-on
/// closed by its note-off, a retrigger closing the one before, and
/// whatever is still down closed at `until`. Other messages are left
/// out, and so are other channels when `only` names one.
pub(crate) fn notes_from(
    midi: &[CapturedMidi],
    from: TimePosition,
    until: TimePosition,
    tempo: &TempoMap,
    only: Option<u8>,
) -> Vec<MidiNote> {
    let start_beat = tempo.samples_to_beat(from).beats();
    let beat_at = |at: TimePosition| tempo.samples_to_beat(at).beats() - start_beat;
    // (channel, note, onset beat, velocity)
    let mut down: Vec<(u8, u8, f64, u8)> = Vec::new();
    let mut notes = Vec::new();
    let mut close = |down: &mut Vec<(u8, u8, f64, u8)>, channel: u8, note: u8, at: f64| {
        if let Some(i) = down.iter().position(|d| d.0 == channel && d.1 == note) {
            let (_, _, on, velocity) = down.remove(i);
            notes.push(MidiNote {
                beat: on,
                length: (at - on).max(1.0 / 960.0),
                note,
                velocity,
                channel,
            });
        }
    };
    let wanted = |m: &&CapturedMidi| match (m.message, only) {
        (
            MidiMessage::NoteOn { channel, .. } | MidiMessage::NoteOff { channel, .. },
            Some(only),
        ) => channel.index() == only,
        _ => true,
    };
    let mut sorted: Vec<&CapturedMidi> = midi
        .iter()
        .filter(|m| m.position >= from && m.position < until)
        .filter(wanted)
        .collect();
    sorted.sort_by_key(|m| m.position);
    for m in sorted {
        let at = beat_at(m.position);
        match m.message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                close(&mut down, channel.index(), note, at);
                down.push((channel.index(), note, at, velocity));
            }
            MidiMessage::NoteOff { channel, note, .. } => {
                close(&mut down, channel.index(), note, at);
            }
            _ => {}
        }
    }
    let end = beat_at(until);
    for (channel, note, on, velocity) in down {
        notes.push(MidiNote {
            beat: on,
            length: (end - on).max(1.0 / 960.0),
            note,
            velocity,
            channel,
        });
    }
    notes.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    notes
}

/// The MIDI clip a recording leaves on `track`: the notes captured
/// between `from` and `until`, named after the track. `None` when no
/// note arrived.
pub(crate) fn midi_clip(
    session: &mut Session,
    track: TrackId,
    midi: &[CapturedMidi],
    from: TimePosition,
    until: TimePosition,
) -> Option<Clip> {
    let only = session
        .memory
        .track(track)
        .ok()
        .and_then(|t| t.state.midi_channel);
    let notes = notes_from(midi, from, until, session.memory.tempo_map(), only);
    let length = TimeDuration::from_samples(until.samples() - from.samples());
    if notes.is_empty() || length.samples() <= 0 {
        return None;
    }
    let name = session
        .memory
        .track(track)
        .map(|t| t.name.clone())
        .unwrap_or_else(|_| "midi".into());
    let id = session.memory.allocate_clip_id();
    Clip::with_id(id, name, from, length, ClipSource::Midi { notes }).ok()
}

/// A file for a new recording on `track` that nothing uses yet, as a
/// path relative to the session folder: `audio/vox 1.wav`,
/// `audio/vox 2.wav`, ... The clip takes its stem as its name. Taken
/// names are those in memory, the files already in the folder, and the
/// files this recording opened so far.
pub(crate) fn audio_key(session: &Session, track: TrackId, opened: &[Entry]) -> Result<PathBuf> {
    let base = session.memory.track(track)?.name.clone();
    let mut n = 1;
    loop {
        let key = PathBuf::from(format!(
            "{}/{base} {n}.wav",
            crate::session::folder::AUDIO_DIR
        ));
        let in_memory = session.audio.get(&key).is_some();
        let on_disk = session.folder.join(&key).exists();
        let opening = opened.iter().any(|e| e.key == key);
        if !in_memory && !on_disk && !opening {
            return Ok(key);
        }
        n += 1;
    }
}
