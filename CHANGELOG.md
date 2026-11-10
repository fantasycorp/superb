# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing has been tagged yet. Everything below is what the first release,
0.1.0, will hold.

### Fixed

- A device output that fails to open no longer loses the engine:
  `AudioThread::run_on_device` hands the audio thread back with the error,
  and `devices::output` keeps it in the session.
- The saved playhead is no longer overwritten by a state change made
  before the engine has applied a seek: `EngineHandle::position` reports a
  pending seek's target.
- A pan law set on a live strip survives a rebuild of the mixer.
- The README quick start asserted a peak the session does not reach.
- A CLAP host test that created plugin instances outside the tests'
  shared lock made `cargo test` fail intermittently.

### Changed

- `AudioThread::run_on_device` returns
  `Result<LiveOutput, (Box<AudioThread>, SuperbError)>`.
- Every Rust example in the Markdown is a whole program, compiled by
  `cargo test --doc --features hardware,files`.
- `/llms.txt` and `/llms-full.txt` carry the flow pages and every example
  program in full.
- [DESIGN.md](DESIGN.md) is the one list of remaining work.
- The documentation site uses bun, like `www/`.

### Added

- **Core Architecture**: Two-tier design with flows (high-level API) and primitives (engine)
- **Session Management**: Create, open, save sessions with JSON persistence and undo history
- **Tracks**: Audio, MIDI, and bus tracks with volume, pan, mute, solo, arm, monitor, input, and MIDI channel
- **Clips**: Audio and MIDI clips with add, move, split, trim, gain, fade_in, fade_out, mute, archive, restore
- **Transport**: Play, stop, seek, rewind, loop support with sample-accurate timing
- **Mixer**: Real-time-safe mixer with tracks, buses, sends, insert chains, pan laws, and peak meters
- **MIDI**: Software and hardware MIDI input with note playback and message routing
- **Recording**: Audio and MIDI recording with punch in/out, streaming to disk, crash recovery
- **Metronome**: Click track with configurable level and count-in bars
- **Export**: Bounce to WAV with resampling, stem sets with loudness analysis
- **Devices**: Audio device enumeration and streaming through cpal, MIDI input through midir
- **Plugin Hosting**: CLAP plugin hosting (VST3 is a stub)
- **Automation**: Envelopes with read/touch/latch/write modes (primitives only)
- **Analysis**: True peak, RMS, LUFS loudness meters, silence detection, recording alignment (primitives only)
- **Dynamics**: Compressor, limiter, gate, de-esser, and leveler as graph nodes (primitives only)
- **File Support**: WAV import/export, comprehensive audio format decoding via symphonia (feature `files`)
- **Real-time Safety**: Zero heap allocation in audio thread proven by `CountingAllocator` tests

### Technical Highlights

- Real-time-safe processing with `prepare`/`process` contract
- Wait-free SPSC queues for thread communication
- Atomic parameter handles for glitch-free parameter changes
- Live graph swapping for structural changes without audio dropouts
- Sample-accurate timing with `TimePosition` and `TimeDuration`
- Musical time with `BeatPosition` and tempo map support
- GPL-free: dual-licensed Apache-2.0 OR MIT
