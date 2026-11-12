//! Recordings a crash left unfinished, and placing them.

use super::*;

/// Recordings a crash left unfinished in the session folder: files that
/// still have their `.recording` tag beside them, as keys (`audio/vox
/// 1.wav`). [`recover`] places them; until then they are not clips and
/// `session::open` only notes them in the log.
pub fn unfinished(session: &Session) -> Vec<PathBuf> {
    steps::tags(&session.folder)
        .into_iter()
        .map(|(_, tag)| tag.key)
        .collect()
}

/// Places the recordings a crash left unfinished ([`unfinished`]): each
/// file's header is finished in place (`wav::repair`), its audio loaded,
/// and a clip placed on the track it was recording for, from the punch-in
/// point the tag remembers, lined up as if the capture began where the
/// transport rolled (the device clock that would say otherwise is gone
/// with the crash). The tags are removed; the files stay, and the
/// placement is one undo step called "Recover" whose undo removes the
/// clips but deletes nothing. A tag whose track is gone, or whose file
/// cannot be read, is left where it is and noted in the log. Refused while
/// a recording runs.
///
/// Made of: the tags in `audio/` → `wav::repair` → `wav::read` into
/// `AudioCache::insert_shared` → `AddClip` through `session::structural`
/// per file, inside one undo group; rebuilds the mixer.
pub fn recover(session: &mut Session) -> Result<Vec<ClipId>> {
    if session.recording.is_some() {
        return Err(SuperbError::InvalidState {
            why: "stop the recording before recovering older ones",
        });
    }
    let tags = steps::tags(&session.folder);
    if tags.is_empty() {
        return Ok(Vec::new());
    }
    let rate = session.memory.sample_rate();
    session.history.begin_group("Recover");
    let mut clips = Vec::new();
    for (tag_file, tag) in tags {
        let path = session.folder.join(&tag.key);
        let track = TrackId::from_raw(tag.track);
        if session.memory.track(track).is_err() {
            log::warn!(
                "{}: its track {} is gone; left as it is",
                tag.key.display(),
                tag.track
            );
            continue;
        }
        let frames = match wav::repair(&path) {
            Ok(frames) => frames,
            Err(e) => {
                log::warn!("{}: cannot repair: {e}", tag.key.display());
                continue;
            }
        };
        let punch_in = TimePosition::from_samples(tag.punch_in);
        let punch_out = tag.punch_out.map(TimePosition::from_samples);
        let Some((start, offset, length)) =
            steps::placement(tag.started_at, frames, punch_in, punch_out)
        else {
            log::info!(
                "{}: nothing of it inside the recording; tag removed",
                tag.key.display()
            );
            if frames == 0 {
                let _ = std::fs::remove_file(&path);
            }
            let _ = std::fs::remove_file(&tag_file);
            continue;
        };
        let (audio, file_rate) = match wav::read(&path) {
            Ok(read) => read,
            Err(e) => {
                log::warn!("{}: cannot read: {e}", tag.key.display());
                continue;
            }
        };
        if file_rate != rate {
            log::warn!(
                "{}: at {} Hz, the session is at {} Hz; left as it is",
                tag.key.display(),
                file_rate.hz(),
                rate.hz()
            );
            continue;
        }
        session.audio.insert(tag.key.clone(), audio);
        let id = session.memory.allocate_clip_id();
        let name = tag
            .key
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let placed = Clip::with_id(
            id,
            name,
            start,
            length,
            ClipSource::Audio {
                path: tag.key.clone(),
                offset,
            },
        )
        .and_then(|clip| session::structural(session, AddClip::new(track, clip)));
        if let Err(e) = placed {
            session.history.cancel_group(&mut session.memory)?;
            return Err(e);
        }
        let _ = std::fs::remove_file(&tag_file);
        clips.push(id);
    }
    session.history.end_group();
    session.needs_rebuild = true;
    Ok(clips)
}
