//! A recording's files: one WAV per armed audio track, the tag beside
//! each that lets a crash be recovered, and the undo step that owns them.

use super::*;

/// The files a recording writes as it goes: one per armed audio track,
/// each holding the channels the track's [`Input`] selects, plus a tag
/// file beside each (`audio/vox 1.wav.recording`) that says where the clip
/// will go, so [`recover`] can place it after a crash. Flushed every
/// `flush_every` frames.
pub(crate) struct Files {
    pub(super) entries: Vec<Entry>,
    /// Device-clock stamp of the first block, once one arrived.
    pub(super) first_us: Option<u64>,
    pub(super) frames: usize,
    pub(super) since_flush: usize,
    pub(super) flush_every: usize,
    /// Silence for a channel the capture does not have.
    pub(super) silence: Vec<f32>,
    pub(super) health: Arc<Health>,
}

/// One file of a recording.
pub(crate) struct Entry {
    pub(super) track: TrackId,
    /// `audio/<track> <n>.wav`, relative to the session folder.
    pub(super) key: PathBuf,
    /// The file itself.
    pub(super) path: PathBuf,
    /// Which captured channels it holds, in order.
    pub(super) pick: Vec<usize>,
    pub(super) writer: wav::Writer,
    pub(super) tag: Tag,
}

/// A finished file of a recording.
pub(crate) struct Finished {
    pub(super) track: TrackId,
    pub(super) key: PathBuf,
    pub(super) path: PathBuf,
    pub(super) frames: u64,
}

/// What `audio/<file>.wav.recording` holds while the file is being written:
/// enough to place the clip without the session that was recording.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Tag {
    pub(super) format_version: u32,
    pub(super) track: RawId,
    pub(super) key: PathBuf,
    /// Timeline sample of the file's first frame.
    pub(super) started_at: i64,
    /// Where the clip starts.
    pub(super) punch_in: i64,
    pub(super) punch_out: Option<i64>,
    pub(super) sample_rate: u32,
    pub(super) channels: usize,
    /// Device-clock stamp of the first block, once known.
    pub(super) first_us: Option<u64>,
}

pub(super) const TAG_FORMAT_VERSION: u32 = 1;
/// What follows a recording's file name while it is being written.
pub(super) const TAG_SUFFIX: &str = ".recording";

impl Tag {
    pub(super) fn path_for(file: &Path) -> PathBuf {
        let mut name = file.as_os_str().to_owned();
        name.push(TAG_SUFFIX);
        PathBuf::from(name)
    }

    pub(super) fn write(&self, file: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SuperbError::Message(format!("cannot encode the recording tag: {e}")))?;
        std::fs::write(Self::path_for(file), json)
            .map_err(|e| SuperbError::io("write", Self::path_for(file), &e))
    }

    pub(super) fn read(path: &Path) -> Result<Tag> {
        let json = std::fs::read_to_string(path).map_err(|e| SuperbError::io("read", path, &e))?;
        serde_json::from_str(&json)
            .map_err(|e| SuperbError::format(Some(path), format!("not a recording tag: {e}")))
    }
}

impl Files {
    /// Frames appended so far.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> usize {
        self.frames
    }

    /// Device-clock stamp of the first block, once one arrived.
    pub(crate) fn first_us(&self) -> Option<u64> {
        self.first_us
    }

    /// Writes every waiting block to the files and hands the blocks back to
    /// the input side. A channel the capture lacks is written as silence;
    /// the first block's device-clock stamp is kept and noted in the tags.
    /// Returns the frames appended. Does I/O; never call it on the audio
    /// thread.
    pub(crate) fn append(&mut self, blocks: &mut CaptureConsumer) -> usize {
        let mut appended = 0;
        while let Some(block) = blocks.pop() {
            let frames = block.frames();
            if self.first_us.is_none() {
                self.first_us = Some(block.block_start_us());
                for entry in &mut self.entries {
                    entry.tag.first_us = self.first_us;
                    let _ = entry.tag.write(&entry.path);
                }
            }
            if frames > self.silence.len() {
                self.silence.resize(frames, 0.0);
            }
            let buffer = block.buffer();
            let silence = &self.silence[..frames];
            for entry in &mut self.entries {
                let planes: Vec<&[f32]> = entry
                    .pick
                    .iter()
                    .map(|&c| {
                        if c < buffer.channels() {
                            buffer.channel(c)
                        } else {
                            silence
                        }
                    })
                    .collect();
                // A failed write is counted by the writer; the queue keeps draining.
                let _ = entry.writer.write(&planes);
            }
            self.health.saw(buffer);
            blocks.recycle(block);
            appended += frames;
            self.frames += frames;
            self.since_flush += frames;
            if self.since_flush >= self.flush_every {
                self.flush();
            }
        }
        self.health.frames.store(self.frames, Ordering::Release);
        self.note_errors();
        appended
    }

    /// Pushes every file to disk and seals what is there.
    pub(crate) fn flush(&mut self) {
        for entry in &mut self.entries {
            let _ = entry.writer.flush();
        }
        self.since_flush = 0;
        let sealed = self
            .entries
            .iter()
            .map(|e| e.writer.counters().sealed_frames())
            .min()
            .unwrap_or(0);
        self.health.sealed.store(sealed, Ordering::Release);
        self.note_errors();
    }

    pub(super) fn note_errors(&self) {
        let errors: u64 = self
            .entries
            .iter()
            .map(|e| e.writer.counters().disk_errors())
            .sum();
        self.health.errors.store(errors, Ordering::Release);
    }

    /// Finishes every file's header and removes the tags. Returns what each
    /// holds; the first failure is returned after every file was tried.
    pub(crate) fn finish(self) -> Result<Vec<Finished>> {
        let mut finished = Vec::with_capacity(self.entries.len());
        let mut failed = None;
        for entry in self.entries {
            let Entry {
                track,
                key,
                path,
                writer,
                ..
            } = entry;
            match writer.finish() {
                Ok(frames) => finished.push(Finished {
                    track,
                    key,
                    path: path.clone(),
                    frames,
                }),
                Err(e) => {
                    failed.get_or_insert(e);
                }
            }
            let _ = std::fs::remove_file(Tag::path_for(&path));
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(finished),
        }
    }
}

/// Places a recorded clip and owns its file: applying writes the WAV when
/// it is not there (a redo after an undo deleted it), reverting deletes
/// it, so an undone recording leaves nothing on disk. The audio stays in
/// `Session::audio` either way.
pub(super) struct RecordClip {
    pub(super) add: AddClip,
    pub(super) folder: PathBuf,
    pub(super) key: PathBuf,
    pub(super) audio: Arc<PlanarBuffer>,
    pub(super) rate: SampleRate,
}

impl Command<Memory> for RecordClip {
    fn label(&self) -> &str {
        "Record"
    }

    fn apply(&mut self, memory: &mut Memory) -> Result<()> {
        if !self.folder.join(&self.key).exists() {
            crate::session::folder::write_audio(&self.folder, &self.key, &self.audio, self.rate)?;
        }
        self.add.apply(memory)
    }

    fn revert(&mut self, memory: &mut Memory) -> Result<()> {
        self.add.revert(memory)?;
        match std::fs::remove_file(self.folder.join(&self.key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SuperbError::io("delete", &self.key, &e)),
        }
    }
}
