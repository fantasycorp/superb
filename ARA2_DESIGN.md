# ARA 2 support: design

**Status:** not started. Waits for the plugins flow ([DESIGN.md](DESIGN.md), steps 1 to 3).
**Last updated:** 2026-11-11.

ARA 2 (Audio Random Access) lets a plugin such as Melodyne read whole
clips from the session instead of one block at a time, and send its edits
back. This page says what Superb has to build to host it, and in which
order. Nothing here is built.

## What changed in this revision

The first version of this page rested on four assumptions. Each was
checked on 2026-11-11.

| assumption then | what is true | consequence |
|---|---|---|
| The ARA SDK is proprietary and may block distribution. | Celemony publishes the ARA SDK and the ARA API under Apache-2.0, and has since 2021. | Licence is not the blocker. It was the top risk; it is removed from the risks. |
| ARA has to come in through VST3. | ARA is a companion to a plugin format, and the ARA API ships a binding header for each of VST3, Audio Units and CLAP (`ARAVST3.h`, `ARAAudioUnit.h`, `ARACLAP.h`). | Superb already hosts CLAP, so the first ARA host goes through CLAP. VST3 and AU get ARA when they are hosted. |
| The VST 3 SDK licence has to be accepted before VST3 can be hosted. | The VST 3 SDK is MIT-licensed from version 3.8.0 (October 2025). Using the VST name or logo still follows Steinberg's trademark rules. | VST3 hosting is no longer held by licence. It is step 2 of the build order. |
| Tempo ramps may come later and need a representation. | Tempo ramps are built (`session::tempo_change` with `TempoChange::Ramp`). | The musical context has to carry ramps from the first version. |

Not verified: the exact identifier a CLAP plugin uses to offer its ARA
factory, and how many shipping ARA plugins offer it through CLAP today.
Both are read from `ARACLAP.h` and from real plugins in the spike below.

One rule of this repository still applies whatever the licences say: all
code is original. The ARA interface is a C API of structs of function
pointers. Superb writes its own declarations of it from the published
specification, as it would for any C ABI, and copies nothing.

## What ARA is

ARA is not a plugin format. A plugin is still loaded as CLAP, VST3 or
AU, and still processes audio block by block as an insert. ARA is a
second interface the same plugin offers beside that one:

| | an insert | the ARA side of the same plugin |
|---|---|---|
| sees | the block it is handed | every clip the host shows it, at any position |
| knows | the transport, at this block | tracks, clips, tempo, meter, markers |
| works | on the audio thread | on its own threads, ahead of playback |
| changes | its parameters | its own analysis and edits, saved with the session |

So an ARA host is two things: the plugin host that exists for inserts,
and a description of the session that the plugin can read.

## What the host provides

ARA calls the host's description of the session a document. Each part
maps onto something the memory already holds:

| ARA | Superb | notes |
|---|---|---|
| document | the `Memory` of a `Session` | one document per open session |
| musical context | the tempo map: tempo changes, ramps, meter changes | read from `Memory::tempo_map()` |
| region sequence | a `Track` | audio tracks only, at first |
| audio source | one audio key in `session.audio` (`audio/vox 1.wav`) | the plugin reads samples from it at any position |
| audio modification | the plugin's own edit of a source | owned by the plugin; the host stores its bytes |
| playback region | a `Clip`: start, length, offset into the source | follows `clip::move_to`, `split`, `trim` |

The host also answers three calls the plugin makes:

* **Read audio.** Samples of a source at any position, from a thread that
  is not the audio thread. `session.audio` holds whole buffers behind an
  `Arc`, so this is a copy out of memory. A recording in progress is not
  offered until `record::stop` has placed its clip.
* **Archive and restore.** The plugin's analysis and edits as bytes,
  saved beside the session and handed back on open.
* **Content changed.** The plugin says its output for a range is
  different now; the host renders that range again.

And the host tells the plugin about every edit: a clip moved, split or
archived, a tempo change, a track renamed. ARA wants these wrapped in
"begin editing" and "end editing" calls.

## Where it sits in Superb

```text
 flows        insert (a plugin insert)            ara (attach, detach)
 ────────────────────────────────────────────────────────────────────
 primitives   plugins::clap   PluginNode          plugins::ara
              the block-by-block side             the document side
                                                    │ reads
                              session.audio ◄───────┤
                              Memory (tracks, clips, tempo map)
```

* `primitives::plugins::ara` holds the declarations of the ARA C
  interface, the host's side of it, and the mapping from `Memory`. It
  needs `unsafe`, so it joins `realtime::{alloc, spsc}` and
  `plugins::clap` as a module that may use it, behind a feature `ara`,
  off by default.
* The audio path does not change. A plugin with ARA is still a
  `PluginNode` in a strip's insert chain; what ARA adds happens off the
  audio thread.
* Edits reach the plugin from one place. Every edit already goes through
  `session::edit` or `session::structural`, and every undo and redo
  through `session::undo` and `redo`. The notification to the plugin is
  made there, by comparing the document before and after, so no verb has
  to remember it.

## Decisions to make first

These are open. Each needs a real plugin on the bench to settle.

| question | leaning |
|---|---|
| Where does an ARA plugin attach: a clip, a track, the session? | A track, as its first insert. It is what a musician does in other hosts, and it reuses the insert chain. |
| Whose undo is it when the plugin edits a note? | The plugin's. Its edits stay in its own history and its archive; Superb's history holds only attaching and detaching. Mixing the two is where hosts desync. |
| Where is the plugin's archive saved? | A file per plugin instance in the session folder, named by `session::Layout`, not inside `memory.json`. It can be megabytes. |
| What does a session do when the plugin is missing? | Opens, keeps the archive untouched, and plays the clips unprocessed. |
| How does the plugin's window open? | Superb has no UI. The flow returns what the application needs to embed the plugin's view; it never opens a window itself. |
| Does a plugin instance survive a rebuild of the mixer? | Settled: it does. The instance is handed from the old mixer to the new one at the swap (`Node::carry`). |

## Build order

ARA comes after the plugins flow, because an ARA plugin is first of all
a loaded plugin with state that outlives a rebuild.

| # | what | done when |
|---|---|---|
| 0 | The plugins flow: CLAP plugins as inserts, instances that outlive a rebuild, state saved with the session | built; what is left of it is [DESIGN.md](DESIGN.md) step 1 |
| 1 | Spike: declare the ARA interface, find the ARA factory of one real CLAP plugin, create a document with one audio source, answer its audio reads | the plugin shows the audio in its own window |
| 2 | The document from the memory: tracks, clips, tempo map with ramps and meter changes; every edit and undo reported | moving, splitting and archiving a clip, and a tempo ramp, are followed by the plugin |
| 3 | Playback: the plugin renders its clips through its insert, in time with the transport, with its latency compensated | a bounce matches what is heard |
| 4 | Persistence: archive on save, restore on open, a missing plugin | a session reopens with the plugin's edits; without the plugin it still opens |
| 5 | The `ara` flow, its page, its reference and an example | the checks in CONTRIBUTING pass |
| 6 | ARA through VST3 and Audio Units | after each is hosted ([DESIGN.md](DESIGN.md) steps 2 and 3) |

## Testing

* The mapping from `Memory` to the document, and the report of edits, are
  plain data and are tested without a plugin.
* An in-process ARA test plugin, like the CLAP test plugin the host is
  tested against today, covers creating a document, reading audio,
  archiving and restoring. No shared library is needed.
* Reading audio for the plugin never runs on the audio thread, and the
  insert path keeps its no-alloc test.
* A real plugin is checked by hand. The tests cannot stand in for that.

## Risks

| risk | what limits it |
|---|---|
| The host's document and the plugin's drift apart after undo | One place reports edits, by comparing before and after; a test replays a history of edits and undos against the test plugin. |
| Few plugins offer ARA through CLAP | The spike finds out first. If so, ARA waits for VST3 hosting, and steps 1 to 5 move behind it unchanged. |
| A plugin instance that must outlive rebuilds | Built and tested in the plugins flow. |
| Unsafe FFI in a second module | The interface is declared once, in one module, with each unsafe block stating what it relies on, as `plugins::clap` does. |
| Work that only a real plugin can verify | Step 1 is small on purpose, and nothing else starts before it works. |

## Not in scope

* A plugin window. The application embeds the plugin's view.
* ARA for MIDI clips or buses.
* Showing the plugin's analysis (notes, tempo) in the memory. ARA lets a
  host read it; nothing here uses it yet.
* AAX.

## References

- [ARA SDK](https://github.com/Celemony/ARA_SDK) and [ARA API](https://github.com/Celemony/ARA_API), Celemony, Apache-2.0
- [Celemony releases the ARA SDK under an open source licence (2021)](https://www.kvraudio.com/news/celemony-releases-ara-sdk-under-open-source-license-51704)
- [VST 3 licensing](https://steinbergmedia.github.io/vst3_dev_portal/pages/VST+3+Licensing/VST3+License) and [VST 3.8.0 SDK released](https://forums.steinberg.net/t/vst-3-8-0-sdk-released/1011988), Steinberg
- [Remaining work](DESIGN.md), [architecture](docs/ARCHITECTURE.md), [the plugin host](docs/primitives/plugins.md)
