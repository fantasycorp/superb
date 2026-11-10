# ARA 2 Plugin Support - Design Document

**Status:** SHELVED / NOT PLANNED FOR IMMEDIATE IMPLEMENTATION  
**Last updated:** Tue Nov 10 2026  
**Owner:** TBD  
**Target milestone:** Post-plugins-flow

## Table of Contents

- [Executive Summary](#executive-summary)
- [Why ARA 2 is Different](#why-ara-2-is-different)
- [Goals](#goals)
- [Non-Goals](#non-goals)
- [Architecture Overview](#architecture-overview)
- [Document Model Mapping](#document-model-mapping)
- [Required Changes](#required-changes)
- [Integration Strategy](#integration-strategy)
- [Lifecycle & Threading](#lifecycle--threading)
- [Undo, State & Persistence](#undo-state--persistence)
- [API Design Sketch](#api-design-sketch)
- [Feature Flags & Dependencies](#feature-flags--dependencies)
- [Licensing & Legal Considerations](#licensing--legal-considerations)
- [Testing Strategy](#testing-strategy)
- [Risks & Mitigations](#risks--mitigations)
- [Open Questions](#open-questions)
- [Implementation Plan (if un-shelved)](#implementation-plan-if-un-shelved)
- [References](#references)

## Executive Summary

ARA 2 (Audio Random Access) enables sample-accurate, document-level communication between a DAW host and plugins (e.g. Celemony Melodyne). Unlike insert effects that process block-by-block with only the buffers provided, ARA plugins need random access to audio across the timeline, knowledge of the project structure (tracks/clips/regions/takes/tempo/markers), and bidirectional edit synchronization.

This document outlines how ARA 2 could be integrated into Superb. Due to complexity (SDK licensing, document model mapping, threading, undo integration), this is **shelved** for now. Recommended path: implement the **plugins flow** first, then prototype **ARA via ARA-capable VST3** behind a new `ara2` feature flag.

## Why ARA 2 is Different

| Aspect | Current Superb (CLAP/VST3 inserts) | ARA 2 |
| --- | --- | --- |
| Processing model | Real-time `Node`/`BlockProcessor` in `primitives::graph`, attached via `Mixer::add_insert`. | Often works with the host document; may still process audio, but core value is offline/async analysis and document-aware editing. |
| Audio access | Sees only per-block buffers passed by the graph. | Needs random access to arbitrary regions/takes across the timeline (and often stable file-backed access). |
| Timeline context | Tempo/map exist in `primitives::transport/session`, but not exposed as a rich host document model to insert nodes. | Requires tempo/meter, time signature, markers, structure, selection, playhead state, and change notifications. |
| Attachment point | Track/bus insert chain. | Typically document-level or track/region-level attachment, negotiated per plugin. |
| Lifecycle | Load → instantiate → prepare → process. | Requires ARA Document Controller, Host/Plug-in model binding, capability negotiation, and often background analysis threads. |
| Edits | Host-driven; plugin state separate. | Plugin-initiated edits must round-trip into the host's session/undo model to remain consistent. |

## Goals

- Define a clean integration strategy that doesn't compromise Superb's real-time safety guarantees.
- Map Superb's `Session`/clips/undo to ARA 2's document model.
- Prefer an incremental approach (ARA-capable VST3) over a standalone loader.
- Gate behind an opt-in feature with clear capability detection.
- Keep integration isolated from the core insert processing path.

## Non-Goals

- Replacing the existing CLAP insert path.
- Full VST3 hosting implementation (separate from this doc; see [DESIGN.md](DESIGN.md)).
- Supporting every ARA 2 plugin on day one.
- Exposing ARA functionality in the default build.

## Architecture Overview

ARA 2 requires host-implemented controllers (commonly): Host Application, Document Controller, Playback Controller, and related model objects. Superb would need analogous components:

```
┌─────────────────────────────────────────────────────────────┐
│ Flows/API (future plugins flow)                              │
└────────────────────────────┬────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────┐
│ ARA Host Layer (primitives/plugins/ara/*)                    │
│ - Document Controller  - Playback Controller                 │
│ - Model mapping/adapters - Change notification bridge        │
└────────────────────────────┬────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────────┐
        ▼                    ▼                        ▼
   Session/Clips      IO/Audio Cache        Transport/Tempo
   (primitives/session) (primitives/io)      (primitives/transport)
```

Key principle: **document-level integration, separate from `PluginNode`/block processing**. Real-time insert processing must remain unaffected.

## Document Model Mapping

| ARA Concept | Superb Equivalent | Notes |
| --- | --- | --- |
| Document | `Session` (memory + layout) | Top-level container; needs stable identity and lifecycle tied to session open/close/save. |
| Musical/Timeline Context | `primitives::transport::tempo_map`, markers (`marker` flow), `TimePosition/BeatPosition` | Must provide tempo/meter changes and timeline structure; ramps (if added later) need representation. |
| Tracks | `Track` in `primitives::session` (Audio/MIDI/Bus) | Map kinds appropriately; buses/master need consideration. |
| Regions/Clips | `Clip` (`primitives::session`) | Need to expose regions with stable IDs, start/end/offset, gain/fades where relevant, and archive state. |
| Audio Sources/Takes | `session.audio` cache + recorded takes on disk | ARA needs random access; must work with streamed recordings and imported files. Provide file paths or readable audio sources. |
| Selection/Playhead | Transport state + UI selection (if exposed) | Expose current playhead/loop range and selection to ARA controllers. |
| Edits/Undo | `primitives::undo` stack + session commands | Plugin-initiated changes must be mappable to undoable operations or go through a host-edit API that integrates with Superb's undo model. |

## Required Changes

| Area | Changes | Priority |
| --- | --- | --- |
| `primitives/plugins/` | Add `ara/` module with host controllers, FFI bindings (behind feature), plugin instance wrapper distinct from `PluginNode`. | High |
| `primitives/session` | Add read-only/document-view APIs for ARA (tracks/clips/regions/takes/markers/tempo), change hooks/notifications, and a host-edit bridge for ARA-initiated modifications. May need stable UUIDs/IDs exposed safely. | High |
| `primitives/io` / cache | Ensure random-access reads for all ARA-exposed audio (disk-streamed takes, cached imports). Consider read-only access guarantees and file locking. | High |
| `primitives/transport` | Expose transport/tempo/map state and change notifications to ARA playback controller. | High |
| `primitives/mixer` / graph | No change to real-time block contract. Ensure ARA instances don't force allocations on audio thread and are not coupled to insert-chain rebuild assumptions. | Medium |
| `src/` (flows) | Extend or add plugins flow APIs: discover ARA-capable plugins, attach/detach at document/track level, manage ARA instances, surface plugin UI hooks. | High |
| `primitives/undo` | Evaluate how to coalesce/map ARA edits into undo groups/steps consistent with existing undo semantics. | High |

## Integration Strategy

**Recommended: ARA via ARA-capable VST3 (phased approach)**

1. **Phase 0 (prerequisite):** Implement the plugins flow and proper insert-chain wiring/loading from session state ([DESIGN.md](DESIGN.md) items 1).
2. **Phase 1 (design/spike):** Complete document model mapping and change-notification design against a real ARA plugin API surface (without full FFI commitment).
3. **Phase 2 (VST3+ARA):** Extend `primitives/plugins/vst3` to detect and initialize ARA extensions for VST3 plugins. Implement minimal ARA host controllers. This reuses format discovery/loading where possible.
4. **Phase 3 (generalize):** If needed, generalize ARA host layer to support other formats carrying ARA. Only consider standalone ARA loading if a concrete use case requires it.

This avoids building a parallel plugin loader ecosystem and matches how most commercial ARA plugins are distributed.

## Lifecycle & Threading

| Concern | Design |
| --- | --- |
| Real-time safety | ARA analysis/UI/editing must run off the audio thread. Never allocate or block arbitrarily on `process()`. Follow Superb's "allocate in prepare, never after" rule. |
| Background work | Use worker threads/tasks for ARA analysis, peak building, etc. Coordinate with session state safely (snapshots or versioned views). |
| Change propagation | Host document changes (edits, tempo changes, comping) must notify ARA controllers. Plugin changes must come through a host-edit queue processed on the appropriate thread with undo integration. |
| Latency | ARA can have different latency characteristics than inserts; surface this separately from insert latency compensation. |
| Thread affinity | Respect ARA SDK threading requirements; isolate in the ARA host layer. |

## Undo, State & Persistence

- **Undo integration:** Route ARA-initiated edits through Superb's command/undo system where feasible. For complex compound edits, use undo groups.
- **Session persistence:** Decide what ARA plugin state to persist (ARA plug-in state, instance bindings/attachments). Store references in session JSON without breaking sessions when the plugin is missing.
- **Offline/archival:** Archived tracks/clips behavior must remain consistent (ARA attachments should be handled gracefully if audio/regions become unavailable).
- **Crash recovery:** Consider ARA background jobs during crash recovery; ensure no partial/corrupt state is written.

## API Design Sketch

> Sketch only - to be refined during implementation. Follow existing flow patterns (free functions over `&mut Session`, `Result<T>`, undo semantics documented).

```rust
// Pseudocode - not final
// In future plugins/ara flow or extended plugins flow
pub fn ara_scan(session: &mut Session) -> Result<Vec<AraPluginInfo>> { /* ... */ }
pub fn ara_attach(session: &mut Session, track: TrackId, plugin_id: &AraPluginId) -> Result<AraInstanceId> { /* ... */ }
pub fn ara_detach(session: &mut Session, instance: AraInstanceId) -> Result<()> { /* ... */ }
pub fn ara_open_editor(session: &mut Session, instance: AraInstanceId) -> Result<()> { /* ... */ }
```

Attachment points may differ from inserts (document-level vs track-level). The API should reflect ARA's model rather than forcing insert-chain semantics.

## Feature Flags & Dependencies

| Item | Proposal | Notes |
| --- | --- | --- |
| Feature flag | `ara2` (opt-in, default off) | Similar to `files`/`devices`. Must not affect default build/tests. |
| SDK | ARA SDK (from ARA development kits) | Proprietary/licensed. Bindings likely via FFI; may need prebuilt SDK or user-provided paths. |
| VST3 integration | May require VST3 hosting support enabled | If going via ARA-capable VST3, `vst3` feature and SDK considerations apply. |
| Platform support | Start with platforms where ARA SDK is available/testable | Document supported platforms; CI may need to skip if SDK unavailable. |

## Licensing & Legal Considerations

- **ARA SDK license:** Review terms before distributing builds with ARA support or depending on SDK headers/binaries. May have restrictions on redistribution.
- **Plugin licensing:** ARA plugins have their own licenses; host support doesn't change plugin licensing.
- **Superb license:** Remain Apache-2.0 OR MIT compatible. Isolate licensed SDK code behind feature flags; avoid contaminating default build.
- **Third-party bindings:** If using external crates, verify license compatibility.

## Testing Strategy

| Test Type | Approach |
| --- | --- |
| Unit tests | Test document model mapping, ID translation, change notification logic with mocks (no real plugin). |
| Integration tests | Behind `#[cfg(feature = "ara2")]` and `#[ignore]` or env-gated if SDK/plugin required. |
| Real-time safety | Ensure ARA paths don't introduce allocations on audio thread; extend existing no-alloc tests where relevant. |
| Compatibility tests | Test with missing plugins (graceful degradation), archived tracks, session save/open, undo/redo. |
| Manual QA | Required for actual ARA plugins (e.g. Melodyne) across supported platforms. |

## Risks & Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| SDK licensing blocks distribution | High | Keep behind opt-in feature, document requirements, never enable by default; consult licensing before release. |
| Complex document model mapping | High | Spike mapping early, focus on read-only first, add write/edit support incrementally. |
| Undo/state desync | High | Route edits through host-edit API with undo groups; add validation/invariants and tests. |
| Real-time thread violations | High | Strict separation (offload to workers), code review for audio-thread paths, use existing allocator/counting tests. |
| Format coupling (VST3/AU) | Medium | Prefer ARA-via-VST3 first, abstract ARA host layer to reduce coupling. |
| Limited testability without plugins | Medium | Heavy mocking, interface-based design, CI stubs for SDK. |
| Maintenance burden | Medium | Shelf until there's clear demand; implement only minimal viable surface area. |

## Open Questions

- [ ] **SDK access:** Do we have access to ARA 2 SDK headers/binaries for target platforms?
- [ ] **Attachment model:** Should ARA instances be attached at document level, track level, or region/clip level in Superb's API?
- [ ] **Edit mapping:** Which ARA edits can be cleanly mapped to existing Superb operations vs requiring new session commands?
- [ ] **Takes/comping:** How does ARA interact with Superb's clip model and future loop recording/take lanes?
- [ ] **Persistence:** What is the minimal set of ARA state to serialize for reproducible sessions?
- [ ] **UI integration:** How will plugin editors be presented (separate windows) given Superb has no built-in UI?
- [ ] **Tempo ramps:** If tempo ramps are implemented, how are they represented to ARA?

## Implementation Plan (if un-shelved)

### Milestone 1: Prerequisites
- [ ] Complete plugins flow ([DESIGN.md](DESIGN.md))
- [ ] Wire insert chains from session state to mixer rebuilds
- [ ] Audit session/clips/tempo APIs for document-view needs

### Milestone 2: Design Spike
- [ ] Create ARA host interface traits (document/playback/controller)
- [ ] Implement mock ARA host for testing
- [ ] Prototype document model mapping with sample session
- [ ] Define change-notification and host-edit bridge

### Milestone 3: VST3 + ARA Integration
- [ ] Add `ara2` feature flag (disabled by default)
- [ ] Integrate ARA SDK FFI (behind feature, isolated)
- [ ] Extend VST3 backend to detect ARA-capable plugins and initialize ARA
- [ ] Implement minimal document/playback controllers
- [ ] Audio source provision via `primitives::io` cache/random access

### Milestone 4: API, Undo & Persistence
- [ ] Add flow APIs for ARA attach/detach/editor
- [ ] Integrate ARA edits with undo system
- [ ] Session serialization for ARA attachments/state
- [ ] Graceful handling of missing plugins

### Milestone 5: Testing & Hardening
- [ ] Mock-based tests for mapping/notifications
- [ ] Real-time safety verification
- [ ] Manual testing with at least one ARA-capable plugin
- [ ] Documentation (flows docs, examples if applicable)
- [ ] Licensing review complete

## References

- [ARA SDK Documentation (Celemony)](https://www.celemony.com/en/service1/sdk)
- [Superb Architecture](docs/ARCHITECTURE.md)
- [Superb Design - Remaining Work](DESIGN.md)
- [Plugins Primitives](src/primitives/plugins/)
- [VST3 Stub](src/primitives/plugins/vst3.rs)
- [Session/Memory Primitives](src/primitives/session/)
- [Mixer Primitives](src/primitives/mixer/)
