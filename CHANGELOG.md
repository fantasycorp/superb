# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `plugin::changes`: what a plugin changed itself (a control turned in
  its own view), as `PluginChange::Touched`, `Value` and `Released`. A
  reported value goes into the memory as an edit. Underneath,
  `PluginInstance::drain_changes` and `PluginNode::take_change_receiver`;
  the CLAP host no longer drops the events a plugin sends out.
- `ParamChange::frame`: a parameter change lands on a frame inside the
  block (`ParamChange::new(id, value).at(frame)`,
  `PluginInstance::set_param_at`). The CLAP host hands its events to the
  plugin in time order.
- `Layout::plugins` and `Layout::plugin_state`: `session::save` writes
  each plugin's state to `plugins/<insert id>.state` instead of into
  `memory.json`, and `open` reads it back. State inside an older
  `memory.json` still loads.
- The `plugin` flow: `plugin::scan`, `params`, `param`, `value` and
  `missing`. A CLAP effect goes on a track with
  `insert::add(.., Effect::Plugin(..))`, is loaded once, and keeps running
  across every rebuild of the mixer; its state is saved with the session,
  a bounce runs a copy of it, and a plugin that is missing does not stop a
  session from opening. `Session::plugin_loader` is the place to add
  another format.
- `Node::carry` and `AudioGraph::adopt_from`: a node hands what it holds
  to the node of the same key in the graph that replaces it, at the swap,
  without allocating. `PluginNode::carried_as`, `adopting` and
  `latency_handle` use it.
- `PluginControl`, from `PluginInstance::control`: the main-thread side of
  a plugin that is processing, to read a parameter and save or restore
  its state. `clap::ClapControl` implements it.
- The `insert` flow: `insert::add`, `remove`, `move_to`, `param`, `bypass`
  and `reduction` put the built-in dynamics (`Effect::Compressor`,
  `Limiter`, `Gate`, `DeEsser`, `Leveler`) on a track, a bus or the master.
  Stored inserts are built into the mixer on every rebuild, the master can
  carry inserts, and a parameter change is heard without a rebuild.
- `SuperbError` says what went wrong: `Io` (with the path and the
  operating system's error kind), `Format`, `DeviceNotFound`, `Device`,
  `MissingAudio`, `InvalidState` and `Unsupported`.

### Changed

- `session::set_tempo` is `session::tempo`; `session::set_loop` is
  `session::loop_range`.
- `send::enable` is `send::on`; `metronome::enable` is `metronome::on`.
- `TrackSend::pre_fader: bool` is `TrackSend::tap: SendTap`, the builder
  is `.tap(SendTap::PreFader)`, and `set_send_pre_fader` is `set_send_tap`.
  Session files are unchanged.
- `Insert::plugin: PluginRef` is `Insert::effect: Effect`. Session files
  written before still load.
- `PluginNode::instance`, `instance_mut` and `into_instance` return an
  `Option`: a node may be waiting for its instance, or have handed it on.
- Bypassing a plugin insert no longer rebuilds the mixer, and
  `insert::add` refuses a plugin that cannot be loaded.
- An insert's bypass is state: saved in the state file, not in
  `memory.json`, and not an undo step.
- Errors that were `SuperbError::Message` are now the variant that fits;
  code matching on `Message` has to match the new one.
- `src/record.rs` is the folder `src/record/`; `mixer/console.rs` is
  `mixer/routing.rs`. No public path changed.

### Removed

- The undoable commands for state: `set_track_mute`, `set_track_solo`,
  `set_track_arm`, `set_track_monitor`, `set_clip_mute`, `set_send_enabled`
  and `set_insert_bypass`, with their coalesce keys. State is set on the
  object's `state` and never enters the undo history.

## [0.1.1] - 2026-11-11

The first published release.

### Added

- Tempo ramps through the flows: `session::tempo_change` takes
  `TempoChange::Step(bpm)` or `TempoChange::Ramp(bpm)`, and a ramp can
  begin at beat zero. The click and MIDI clips follow.

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

- `session::tempo_change` takes `Option<TempoChange>` where it took
  `Option<f64>`: `Some(120.0)` becomes `Some(TempoChange::Step(120.0))`.
- `session::set_tempo` keeps a ramp that begins at the start.
- `AudioThread::run_on_device` returns
  `Result<LiveOutput, (Box<AudioThread>, SuperbError)>`.
- Every Rust example in the Markdown is a whole program, compiled by
  `cargo test --doc --features hardware,files`.
- `/llms.txt` and `/llms-full.txt` carry the flow pages and every example
  program in full.
- [DESIGN.md](DESIGN.md) is the one list of remaining work.
- The documentation site uses bun, like `www/`.

### Added

- **Test tone**: `PlanarBuffer::sine` makes one channel of a sine at a frequency and level, audio for a session without a file
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
