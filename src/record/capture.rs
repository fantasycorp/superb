//! A recording in progress: what the application reads while it runs,
//! and how its blocks get from the capture queue to its files.

use super::*;

/// A recording in progress: what [`Session::recording`] holds between
/// [`start`] and [`stop`]. Its audio is on disk from the first block: one
/// WAV file per armed audio track in the session folder (`audio/vox
/// 1.wav`), written as the blocks arrive and flushed about once a second,
/// so a crash costs a second of audio, not the recording, and the recording never
/// has to fit in memory. The counters here are what an application shows
/// while it records: frames captured and sealed on disk, blocks dropped,
/// disk errors, the level of the latest block, and how long ago it came.
pub struct Recording {
    /// The audio capture, when an audio track is armed.
    pub(super) capture: Option<Capture>,
    /// Incoming MIDI gathered so far, for the armed MIDI tracks.
    pub(super) midi: Vec<CapturedMidi>,
    pub(super) started_at: TimePosition,
    pub(super) punch_in: TimePosition,
    pub(super) punch_out: Option<TimePosition>,
    pub(super) tracks: Vec<TrackId>,
    pub(super) channels: usize,
    /// `EngineHandle::roll_start().rolls` before the transport was told to
    /// roll, so `stop` can tell this recording's roll start from an older one.
    pub(super) rolls_before: u64,
    /// The files being written, as keys into the session folder.
    pub(super) keys: Vec<PathBuf>,
    /// What the capture has done so far, shared with the thread that writes.
    pub(super) health: Arc<Health>,
}

impl Recording {
    /// Frames captured so far: handed to the files, not all on disk yet.
    pub fn frames(&self) -> usize {
        self.health.frames.load(Ordering::Acquire)
    }

    /// Frames on disk for good in every file of the recording: written,
    /// synced and covered by a header a reader accepts. Everything before
    /// this frame survives a crash and will never change, so an uploader
    /// can ship the byte range it covers. Advances about once a second.
    pub fn sealed_frames(&self) -> u64 {
        self.health.sealed.load(Ordering::Acquire)
    }

    /// Writes that failed across the files (disk full, file gone, I/O
    /// error). Anything above zero means a gap in a file; the recording
    /// keeps draining its queue so the audio thread is never held up, and
    /// the application decides whether to stop.
    pub fn disk_errors(&self) -> u64 {
        self.health.errors.load(Ordering::Acquire)
    }

    /// Blocks the input side could not queue because the queue was full.
    /// Anything above zero means a gap in the recording.
    pub fn dropped(&self) -> u64 {
        self.capture.as_ref().map_or(0, Capture::dropped)
    }

    /// Peak per captured channel of the latest block, 0.0 to 1.0 and
    /// above when clipping; all zero before the first block.
    pub fn peaks(&self) -> Vec<f32> {
        self.health
            .peaks
            .iter()
            .map(|p| f32::from_bits(p.load(Ordering::Relaxed)))
            .collect()
    }

    /// How long ago the last block arrived; `None` before the first. A
    /// value that keeps growing while the transport rolls means the source
    /// went quiet: the device was unplugged, or nothing feeds the queue.
    pub fn since_last_block(&self) -> Option<core::time::Duration> {
        let at = self.health.last_block_us.load(Ordering::Acquire);
        (at != u64::MAX).then(|| {
            self.health
                .since
                .elapsed()
                .saturating_sub(core::time::Duration::from_micros(at))
        })
    }

    /// The files being written, one per armed audio track, as keys into
    /// the session folder (`audio/vox 1.wav`). They exist from [`start`]
    /// on, grow as the recording runs, and are the clips' audio after
    /// [`stop`].
    pub fn files(&self) -> &[PathBuf] {
        &self.keys
    }

    /// Where the transport started rolling: at or before the punch-in.
    pub fn started_at(&self) -> TimePosition {
        self.started_at
    }

    /// Where the recording begins and the clips will start.
    pub fn punch_in(&self) -> TimePosition {
        self.punch_in
    }

    /// Where the recording ends on its own, if a punch-out point was set.
    pub fn punch_out(&self) -> Option<TimePosition> {
        self.punch_out
    }

    /// The tracks that were armed when it started; each gets a clip.
    pub fn tracks(&self) -> &[TrackId] {
        &self.tracks
    }

    /// Audio channels being captured; zero when no audio track is armed.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Incoming MIDI messages gathered so far.
    pub fn midi_messages(&self) -> usize {
        self.midi.len()
    }

    /// Moves waiting blocks into a queue recording's files; see [`collect`].
    pub(crate) fn collect(&mut self) -> usize {
        match self.capture.as_mut() {
            Some(Capture::Manual { blocks, files }) => files.append(blocks),
            _ => 0,
        }
    }

    /// Takes the MIDI the engine captured since the last call.
    pub(crate) fn gather_midi(&mut self, engine: &mut crate::primitives::io::EngineHandle) {
        while let Some(message) = engine.captured_midi() {
            self.midi.push(message);
        }
    }
}

impl core::fmt::Debug for Recording {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Recording")
            .field("started_at", &self.started_at)
            .field("punch_in", &self.punch_in)
            .field("punch_out", &self.punch_out)
            .field("tracks", &self.tracks)
            .field("channels", &self.channels)
            .field("files", &self.keys)
            .field("frames", &self.frames())
            .field("sealed", &self.sealed_frames())
            .field("dropped", &self.dropped())
            .field("disk_errors", &self.disk_errors())
            .field(
                "threaded",
                &matches!(self.capture, Some(Capture::Threaded(_))),
            )
            .field("midi", &self.midi.len())
            .finish()
    }
}

/// What a recording's capture has done so far: written by whichever thread
/// drains the queue, read by the application through [`Recording`].
pub(crate) struct Health {
    pub(super) frames: AtomicUsize,
    pub(super) sealed: AtomicU64,
    pub(super) errors: AtomicU64,
    /// Peak of the latest block per captured channel, as `f32` bits.
    pub(super) peaks: Vec<AtomicU32>,
    /// When the latest block arrived, in microseconds after `since`;
    /// `u64::MAX` before the first.
    pub(super) last_block_us: AtomicU64,
    pub(super) since: std::time::Instant,
}

impl Health {
    pub(super) fn new(channels: usize) -> Arc<Self> {
        Arc::new(Health {
            frames: AtomicUsize::new(0),
            sealed: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            peaks: (0..channels).map(|_| AtomicU32::new(0)).collect(),
            last_block_us: AtomicU64::new(u64::MAX),
            since: std::time::Instant::now(),
        })
    }

    /// Notes a block: its peaks and when it came.
    pub(super) fn saw(&self, block: &PlanarBuffer) {
        for (ch, peak) in self.peaks.iter().enumerate() {
            let value = if ch < block.channels() {
                block.channel(ch).iter().fold(0.0f32, |m, s| m.max(s.abs()))
            } else {
                0.0
            };
            peak.store(value.to_bits(), Ordering::Relaxed);
        }
        self.last_block_us
            .store(self.since.elapsed().as_micros() as u64, Ordering::Release);
    }
}

/// How a recording's blocks get from the capture queue into its files.
pub(crate) enum Capture {
    /// Blocks wait on the queue until [`collect`] writes them.
    Manual {
        /// The queue's consumer end.
        blocks: CaptureConsumer,
        /// The files being written.
        files: Files,
    },
    /// A thread empties the queue into the files as it fills; nothing to
    /// call. Only a device source makes one, so without `devices` it
    /// exists for tests.
    #[cfg_attr(not(feature = "devices"), allow(dead_code))]
    Threaded(Collector),
}

impl Capture {
    /// Blocks the input side had to drop.
    pub(crate) fn dropped(&self) -> u64 {
        match self {
            Capture::Manual { blocks, .. } => blocks.dropped(),
            Capture::Threaded(collector) => collector.dropped(),
        }
    }
}

/// A thread that writes captured blocks to the recording's files as they
/// arrive, so a recording can be as long as the disk allows no matter how
/// small the queue is. Made by `steps::spawn_collector`;
/// [`finish`](Collector::finish) hands the files back.
pub(crate) struct Collector {
    pub(super) dropped: Arc<AtomicU64>,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) handle: Option<JoinHandle<(Files, CaptureConsumer)>>,
}

impl Collector {
    /// Blocks the input side had to drop (the thread was late, or the
    /// queue is too small for the block rate).
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Stops the thread after it has drained what is left on the queue and
    /// returns the files and the queue's consumer end. Stop the input side
    /// first so nothing new arrives.
    pub(crate) fn finish(mut self) -> Result<(Files, CaptureConsumer)> {
        self.stop.store(true, Ordering::Release);
        let handle = self.handle.take().expect("finish is called once");
        handle
            .join()
            .map_err(|_| SuperbError::Message("the recording collector thread panicked".into()))
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        // Not finished: let the thread exit on its own. Its files stay on
        // disk, unfinished, with their tags; `recover` finds them.
        self.stop.store(true, Ordering::Release);
    }
}

impl core::fmt::Debug for Collector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Collector")
            .field("dropped", &self.dropped())
            .finish()
    }
}
