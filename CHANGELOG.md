# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Documentation improvements and reference generation for all flows

### Changed

- Code cleanup and formatting consistency across the codebase

## [0.1.0] - 2024-10

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

[unreleased]: https://github.com/superb-audio/superb-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/superb-audio/superb-sdk/releases/tag/v0.1.0
