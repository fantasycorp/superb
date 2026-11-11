# transport

Start, stop, seek, loop; where the playhead is.

## Example

```rust
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::transport::LoopRange;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("play"), rate, 256)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize * 2);
    tone.fill(0.25);
    session.audio.insert("audio/tone.wav", tone);
    clip::add(&mut session, vox, "audio/tone.wav", TimePosition::ZERO)?;

    transport::start(&mut session)?;                            // rebuilds the mixer first
    let audio = session::steps::render_blocks(&mut session, 47)?;   // 0.25 s with no sound card
    println!("{:?}, peak {:.3}, at {:.2} s",
        transport::state(&session), audio.peak(),
        transport::playhead(&session).to_seconds(rate));

    transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
    session::steps::render_blocks(&mut session, 1)?;            // commands apply at the next block

    let range = LoopRange::new(TimePosition::from_seconds(0.5, rate), TimePosition::from_seconds(1.0, rate))?;
    session::set_loop(&mut session, Some(range))?;
    transport::looping(&mut session, true)?;                    // state: saved, not an undo step
    transport::seek(&mut session, range.start())?;
    session::steps::render_blocks(&mut session, 141)?;          // 0.75 s inside a 0.5 s loop
    println!("looping at {:.2} s", transport::playhead(&session).to_seconds(rate));

    transport::stop(&mut session)?;
    transport::rewind(&mut session)?;
    Ok(())
}
```

With the `devices` feature the same session plays through the speakers:

```rust,no_run
use superb::primitives::{Result, SampleRate};
use superb::{devices, session, transport};

fn main() -> Result<()> {
    let folder = session::Root::default().folder("My Show");   // ~/superb/My Show/
    let mut session = session::open(&folder, 256)?;
    devices::output(&mut session, None)?;       // the default output drives the engine from here
    transport::start(&mut session)?;
    std::thread::sleep(std::time::Duration::from_secs(10));
    transport::stop(&mut session)?;
    Ok(())
}
```

`cargo run --example play_session` runs both.

## Verbs

| verb | does |
|---|---|
| `start(session)` | play from the playhead; rebuilds the mixer first if the memory changed |
| `stop(session)` | |
| `seek(session, to)` | move the playhead, stopped or rolling |
| `rewind(session)` | seek to zero |
| `looping(session, on)` | loop the range from `session::set_loop`; state |
| `playhead(session) -> TimePosition` | |
| `state(session) -> TransportState` | `Stopped`, `Playing` or `Recording` |

Every signature, argument, error and undo step: [transport reference](transport.ref.md).

## Notes

* Commands apply at the engine's next block, so `state` right after
  `start` still says `Stopped`. Render a block (or wait a few milliseconds
  on a device) before reading.
* A recording with punch points switches `Playing` ↔ `Recording` on its own.
