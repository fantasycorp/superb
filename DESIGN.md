# Remaining work

What Superb does not do yet, and the order to build it in. This is the one
list: [README.md](README.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
link here instead of keeping their own. What works today is in the README's
status section and [CHANGELOG.md](CHANGELOG.md). Each primitive's page under
`docs/primitives/` ends with the limits of that module alone.

Last updated: 2026-11-11.

## Build order

Each step says why it sits where it does. A step is done when it has a
flow verb with a "Made of:" line, a reference note in
`scripts/reference.py`, a page or a row in `docs/`, an example, and, on the
audio path, a no-alloc test.

| # | what | why here |
|---|---|---|
| 1 | Finish the plugins flow for CLAP | Effects work: a CLAP plugin is an insert, loaded once, handed from mixer to mixer at each rebuild, saved with the session. What is left before another format is worth adding: an instrument on a MIDI track, and the host extensions a real plugin asks for (a parameter rescan, a latency that changes, its window). The plugin's own parameter changes are heard (`plugin::changes`). |
| 2 | VST3 hosting | Most plugins a musician owns are VST3. The SDK is MIT-licensed from 3.8.0 (October 2025), so the licence no longer holds it. It lands behind a feature, through the same flow as step 1. |
| 3 | Audio Units hosting | macOS only, through the system's AudioToolbox. Some plugins ship as nothing else there. Behind a feature, through the same flow. |
| 4 | Automation in the memory, with a flow | Envelopes exist as primitives, but the memory owns no lanes. Insert and plugin parameters are its targets, beside volume, pan and send level. For a plugin the pieces it stands on are in place: a `ParamChange` lands on a frame inside a block, and the plugin's gestures (`PluginChange::Touched`, `Released`) are heard for touch and latch. The order on load is fixed: the plugin's state file, then the memory's parameter values, then automation on top. A bounce's copy of a plugin must be driven by the same lanes. |
| 5 | Incremental mixer updates | A rebuild replaces the whole mixer. With plugins in it, a rebuild on every clip drag is expensive even when the instances are kept. |
| 6 | ARA 2 | A plugin reads whole clips and sends edits back. It needs the instances of step 1; it goes through CLAP first, then VST3 and AU. See [ARA2_DESIGN.md](ARA2_DESIGN.md). |
| 7 | Duplex streams, and MIDI on the audio clock | Hardware MIDI lands at block resolution, which is timing error in every MIDI recording. Input and output run as two streams on two clocks. |
| 8 | Loop recording | Builds on the recording that exists. Needs a model for recordings stacked on one range, and a word for them: TERMINOLOGY reserves "take". |
| 9 | MIDI ports per track, then Standard MIDI Files | Every connected port feeds every track today. Files are the way MIDI moves between programs. SysEx and MIDI Time Code are rarer and come last. |
| 10 | Encoders other than WAV, with dither | Bouncing to 16-bit truncates without dither. Any encoder has to pass the licence rule in CONTRIBUTING. |

Plugins come first by decision (2026-11-11). Within them CLAP is first
because its host is written and tested; VST3 and Audio Units reuse the
flow it defines: each is a `PluginInstance` with a `PluginControl`, made
by `Session::plugin_loader`.

## Everything not built

### Plugins and inserts

| item | state | notes |
|---|---|---|
| Instrument plugins on MIDI tracks | not started | A plugin with no audio input is refused as an insert, and a monitoring MIDI track gets `MonoSynthNode`, a stand-in. Needs the plugin as the track's source, fed the track's MIDI. |
| Host extensions for CLAP | partial | The parameter values and gestures a plugin sends out are heard (`plugin::changes`). A plugin still cannot ask the host to rescan its parameters or resize its latency: `host.get_extension` answers nothing. |
| A parameter's value as text | not started | `ParamInfo` gives a range, not "3.2 kHz". CLAP's `value_to_text` is not called. |
| A plugin's window | not started | The flow gives no way to open a plugin's editor. Superb has no UI; the flow has to hand the application what it needs to embed one. |
| Plugins tested by hand | not started | The flow is tested against the host's in-process test plugin and a mock. No third-party CLAP plugin has been run through it. |
| A plugin's latency after it changes | not started | The mixer is laid out with the latency the plugin reported when it was last prepared. |
| Plugin state held in memory | to revisit | Each plugin's state is a file of its own on disk (`plugins/<insert id>.state`), but the bytes of every plugin are also held in the memory, and in the undo history for a removed insert. Large plugins make that heavy. |
| A scan cache | not started | `plugin::scan` loads every bundle each time. |
| Live bypass for the built-in effects | not started | `insert::bypass` rebuilds the mixer without a built-in effect (a plugin is switched live). A bypass inside the node would need to keep the limiter's lookahead delay. |
| VST3 hosting | stub | `primitives::plugins::vst3` returns `unsupported()`. Licence reviewed 2026-11-11: MIT from SDK 3.8.0; the VST name and logo follow Steinberg's trademark rules. The interfaces are declared by hand from the specification (the originality rule), in a module that may use `unsafe`, which CONTRIBUTING has to name. |
| Audio Units hosting | not started | macOS only. AUv2 units load into the host's process; AUv3 units load out of process by default. Needs a module that may use `unsafe`, and a licence check of what it links before it lands. |
| ARA 2 | design only | [ARA2_DESIGN.md](ARA2_DESIGN.md). The ARA API is Apache-2.0 and has bindings for CLAP, VST3 and Audio Units. |

### Mixer and engine

| item | state | notes |
|---|---|---|
| Incremental mixer updates | not started | Clip gain, fades and mute rebuild the whole mixer; so does every undo, even of a fader, and every insert added, moved or bypassed, which resets the built-in effects' envelopes. Plugin instances are kept. |
| Pan law in the memory | not started | A pan law is set on the live strip. A rebuild carries it over, but it is not saved. |
| Multi-bus and sidechain nodes | not started | A node has one input bus and one output bus. |
| Flow for analysis | primitives only | `export::stems` reports loudness; the meters are otherwise yours to call. |

### Automation

| item | state | notes |
|---|---|---|
| Lanes owned by the session | not started | `primitives::automation` has envelopes and modes; the memory stores none, per track or per clip. |
| Automation flow | not started | Add and edit points, modes, write from a moving control. |

### Tempo and time

| item | state | notes |
|---|---|---|
| Hardware MIDI on the audio clock | not started | `primitives::midi::input` takes the backend's timestamps as they are. |

### MIDI

| item | state | notes |
|---|---|---|
| A port per track | not started | A track chooses a channel (`track::midi_channel`), not a port. |
| Disconnecting an input | not started | `midi::connect_input` has no counterpart; the bridge closes when removed from `Session::midi_inputs`. |
| Standard MIDI Files | not started | Import and export, through the tempo map. |
| SysEx | not started | Messages are fixed-size on the audio path. |
| MIDI Time Code | not started | |

### Recording

| item | state | notes |
|---|---|---|
| Loop recording | not started | Punch in and out exist. |

### Devices

| item | state | notes |
|---|---|---|
| Duplex streams | not started | Input and output are separate streams; a recording lines them up by the device clock. |

### Files

| item | state | notes |
|---|---|---|
| Encoders other than WAV | not started | Writing is WAV only. |
| Dither | not started | `Bounce` defaults to 16-bit PCM, `Stems` to 24-bit. |

### The API itself

Found in review, not features.

| item | notes |
|---|---|
| `SendState::enabled`, `MetronomeState::enabled` | State switches are nouns (`mute`, `arm`, `bypass`); the verbs are `send::on` and `metronome::on`, the fields still say `enabled`. Renaming them touches both file formats. |
| The example `record_take` | The word is "recording". |
| `SuperbError::Message(String)` | About ten uses remain: a thread that panicked, a serialiser that failed, a plugin's own error. |

## How to build any of it

- Flows are free functions over `&mut Session`. Edits go through the undo
  history; state goes through `session::set_state` and never does.
- `prepare` may allocate, `process` never does. Until step 5, structure
  changes by building a whole mixer and swapping it in.
- Hardware and codecs stay behind features, off by default.
- Docs, example and tests land with the code. `docs/flows/<flow>.ref.md`
  is generated; every example in the Markdown is compiled by
  `cargo test --doc --features hardware,files`.
