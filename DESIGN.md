# Superb Design: Remaining Work

This document tracks features not yet implemented in Superb. For a shelved proposal on ARA 2 integration, see [ARA2_DESIGN.md](ARA2_DESIGN.md).

Last updated: Tue Nov 10 2026

## Overview

Superb follows a two-tier architecture (flows + primitives). Much of the core DAW functionality is implemented, but several features are either missing entirely or only exist at the primitive level. This document enumerates remaining work as of the current `main` branch.

## Completed (for context)

See [README.md](README.md#status) and [CHANGELOG.md](CHANGELOG.md) for the full list of implemented features. Key areas working today: session/memory/undo, tracks/clips/markers/routing, transport/tempo/meter, mixer with buses/sends, recording (audio/MIDI) with crash recovery, metronome/count-in, export to WAV/stems, device I/O, MIDI messages/channels/hardware input, CLAP plugin hosting primitives, and insert chains/automation/analysis/dynamics at the primitive level.

## Remaining Work

### 1) Plugins & Inserts
| Item | Status | Notes |
| --- | --- | --- |
| Plugins flow (top-level API) | Not started | No `src/plugins.rs` flow. Primitives exist under `primitives/plugins/` (CLAP default, VST3 stub). Need verbs for scanning/loading/instantiating plugins, attaching them to tracks/buses, managing presets/state, etc. |
| Load track-stored inserts on rebuild | Not started | Inserts/chains exist in `primitives/mixer` (`Mixer::add_insert`), but `primitives/session` insert storage is not wired into mixer rebuilds. Documented as missing in `docs/ARCHITECTURE.md`. |
| VST3 hosting | Stub only | `primitives/plugins/vst3.rs` returns `unsupported()`, `AVAILABLE = false`. Full COM/SDK integration required if pursued. |
| ARA 2 support | Shelved (design only) | See [ARA2_DESIGN.md](ARA2_DESIGN.md). Likely via ARA-capable VST3. |

### 2) MIDI
| Item | Status | Notes |
| --- | --- | --- |
| Tell MIDI ports apart per track | Not started | Currently MIDI routing is per-track channel selection; need ability to assign specific hardware/software MIDI ports per track and filter accordingly. |
| SysEx support | Not implemented | Mentioned as unsupported in `primitives/midi`. Need parsing/serialization and safe routing (real-time considerations). |
| MIDI Time Code (MTC) | Not implemented | Referenced as unsupported alongside SysEx. |
| Standard MIDI Files (SMF) import/export | Not started | No SMF read/write. Would need format support, tempo map integration, and flow APIs. |

### 3) Recording
| Item | Status | Notes |
| --- | --- | --- |
| Loop recording (take lanes/comping) | Not started | Punch in/out exists. Need loop recording behavior (multiple takes per clip/region, take management, comp selection). May require session model extensions and undo integration. |

### 4) Tempo & Timing
| Item | Status | Notes |
| --- | --- | --- |
| Tempo ramps (glide/curves) | Not started | Tempo map supports stepped changes only. Need ramp interpolation, sample-accurate application, click/transport/mixer alignment, and possible automation integration. |
| Align hardware MIDI timestamps with the audio clock | Not started | `primitives/midi/input` receives timestamps from backends but does not re-stamp to the audio clock. Needed for tight sync. |

### 5) Audio Devices
| Item | Status | Notes |
| --- | --- | --- |
| Duplex device streams (simultaneous I/O) | Not started | Input and output streams exist separately. Need combined duplex streams, sync, latency reporting, and robust error handling. |

### 6) Mixer & Engine
| Item | Status | Notes |
| --- | --- | --- |
| Incremental mixer updates (vs full rebuild) | Not started | Structural changes trigger a full rebuild and swap. Need differential updates while preserving real-time safety (and handling plugin tails/meters appropriately). |
| Expose insert chains via flows | Partial (primitives only) | Insert chains exist primitively; need flow-level APIs once plugins flow is designed. |

### 7) File I/O & Encoding
| Item | Status | Notes |
| --- | --- | --- |
| Encoders other than WAV | Not started | Export/bounce is WAV-only currently. Need additional encoders behind features as appropriate, respecting licensing constraints. |

### 8) Flow APIs for primitive-only features
| Item | Status | Notes |
| --- | --- | --- |
| Automation flow | Primitives only | Envelopes/modes exist in `primitives::automation`. Consider flow verbs for adding/editing automation, lanes, etc. |
| Analysis flow | Primitives only | LUFS/true peak/RMS/silence/alignment exist. Consider higher-level flow APIs if needed. |
| Dynamics flow | Primitives only | Compressor/limiter/gate/de-esser/leveler exist as nodes. Consider flow-level configuration/presets or document as graph-building utilities. |

## Suggested Priorities

1. **Plugins flow + insert wiring** - Unblocks plugin management and makes existing insert chain/mixer capabilities accessible via flows.
2. **MIDI port routing + SysEx/MTC/SMF** - Common DAW requirements; SMF especially useful for interoperability.
3. **Loop recording** - Extends the existing robust recording system.
4. **Tempo ramps** - Important for musical workflows; touches tempo map, transport, click, and possibly automation.
5. **Duplex streams + MIDI clock alignment** - Improves hardware integration/sync.
6. **Incremental mixer updates** - Performance/UX improvement, but more invasive to the rebuild/swap model.
7. **Additional encoders / VST3** - Format expansion; VST3 has SDK/licensing complexity.
8. **ARA 2** - See [ARA2_DESIGN.md](ARA2_DESIGN.md); keep shelved until plugins flow + document model are solid.

## Implementation Notes

- Follow existing patterns: flows are free functions over `&mut Session`, document with "Made of:" lists, edits go through undo where appropriate, state changes may not.
- Respect real-time safety: any audio-thread changes must follow `prepare`/`process` contracts, avoid allocations on audio thread, prefer graph rebuild/swap patterns until incremental updates are designed.
- Feature-gate hardware/codecs: use existing feature flags (`devices`, `midi-io`, `files`, `clap`) and add new ones as needed (`vst3`, `ara2`, encoders) with non-default/off-by-default where heavy/licensed.
- Update docs/examples/tests alongside implementation. Reference docs are generated from source - keep signatures/docs in sync.

## Handoff Notes for Fable

- Start by reading [README.md](README.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for conventions.
- Check [docs/flows/](docs/flows/) for flow patterns and existing APIs.
- Look at `src/` flows and corresponding `primitives/` modules for how features are split.
- Use this document to scope work; ARA 2 details are intentionally separated in `ARA2_DESIGN.md` to avoid scope creep.
