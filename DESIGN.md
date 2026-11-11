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
| 1 | Inserts on rebuild, then the plugins flow | The memory stores a track's inserts and `build_mixer` ignores them, so nothing stored as an insert is ever heard. Begin with the built-in dynamics as inserts (no FFI), then CLAP. Decide here how a plugin instance outlives a rebuild. |
| 2 | Automation in the memory, with a flow | Envelopes exist as primitives, but the memory owns no lanes. Automation needs insert parameters as targets, so it follows step 1. |
| 3 | Incremental mixer updates | A rebuild replaces the whole mixer. Today that costs a meter reset; once plugins exist it cuts their tails and builds them again on every clip drag. |
| 4 | Duplex streams, and MIDI on the audio clock | Hardware MIDI lands at block resolution, which is timing error in every MIDI recording. Input and output run as two streams on two clocks. |
| 5 | Loop recording | Builds on the recording that exists. Needs a model for recordings stacked on one range, and a word for them: TERMINOLOGY reserves "take". |
| 6 | MIDI ports per track, then Standard MIDI Files | Every connected port feeds every track today. Files are the way MIDI moves between programs. SysEx and MIDI Time Code are rarer and come last. |
| 7 | Encoders other than WAV, with dither | Bouncing to 16-bit truncates without dither. Any encoder has to pass the licence rule in CONTRIBUTING. |
| 8 | VST3, then ARA 2 | The largest and the most constrained by licence. ARA 2 needs a plugins flow and VST3 first; see [ARA2_DESIGN.md](ARA2_DESIGN.md). |

## Everything not built

### Plugins and inserts

| item | state | notes |
|---|---|---|
| Stored inserts loaded on rebuild | not started | `Insert` and its commands are in `primitives::session`; `Mixer::add_insert` is in `primitives::mixer`; `session::steps::build_mixer` does not join them. |
| Plugins flow | not started | Scan, load, place on a track or bus, parameters, presets and state. The host is `primitives::plugins` (CLAP). |
| Instrument plugins on MIDI tracks | not started | A monitoring MIDI track gets `MonoSynthNode`, a stand-in. |
| VST3 hosting | stub | `primitives::plugins::vst3` returns `unsupported()`. The SDK's licence terms have to be reviewed first (CONTRIBUTING). |
| ARA 2 | shelved, design only | [ARA2_DESIGN.md](ARA2_DESIGN.md). |

### Mixer and engine

| item | state | notes |
|---|---|---|
| Incremental mixer updates | not started | Clip gain, fades and mute rebuild the whole mixer; so does every undo, even of a fader. |
| Pan law in the memory | not started | A pan law is set on the live strip. A rebuild carries it over, but it is not saved. |
| Multi-bus and sidechain nodes | not started | A node has one input bus and one output bus. |
| Flows for the dynamics | primitives only | Compressor, limiter, gate, de-esser and leveler are nodes; they become reachable as inserts in step 1. |
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

Found in review, not features. Each breaks a rule in CONTRIBUTING's
"Naming".

| item | notes |
|---|---|
| `session::set_tempo`, `session::set_loop` | Setters are nouns without `set_`. |
| `send::enable`, `metronome::enable` | Every other switch is the noun with `on`: `track::mute(.., on)`. |
| The noun "take" | In `record` docs and TERMINOLOGY itself; the word is reserved. |
| `mixer/console.rs`, "console" in `io::cache` | The word is "mixer". |
| `TrackSend::pre_fader: bool` | `SendTap` exists; the memory should carry the enum. |
| `commands::set_track_mute`, `solo`, `arm`, `monitor` | State never enters the undo history, yet these are undoable commands. |
| `SuperbError::Message(String)` | Used over a hundred times; an application cannot tell "no such device" from "file missing", and I/O errors lose the path. |
| `src/record.rs` | 2,300 lines: flow, capture, files and recovery in one file. |

## How to build any of it

- Flows are free functions over `&mut Session`. Edits go through the undo
  history; state goes through `session::set_state` and never does.
- `prepare` may allocate, `process` never does. Until step 3, structure
  changes by building a whole mixer and swapping it in.
- Hardware and codecs stay behind features, off by default.
- Docs, example and tests land with the code. `docs/flows/<flow>.ref.md`
  is generated; every example in the Markdown is compiled by
  `cargo test --doc --features hardware,files`.
