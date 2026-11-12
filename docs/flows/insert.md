# insert

The effects in a track's signal path, in order: compressor, limiter, gate, de-esser, leveler.

## Example

```rust
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::{Effect, EffectParam, TrackKind};
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, insert, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("inserts"), rate, 256)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize);
    tone.fill(0.5);
    session.audio.insert("audio/tone.wav", tone);
    clip::add(&mut session, vox, "audio/tone.wav", TimePosition::ZERO)?;

    let gate = insert::add(&mut session, vox, Effect::Gate)?;          // first in the chain
    let comp = insert::add(&mut session, vox, Effect::Compressor)?;    // then this, then the fader
    insert::param(&mut session, comp, EffectParam::Threshold, -24.0)?; // dB; heard at once; one undo step per drag
    insert::param(&mut session, comp, EffectParam::Ratio, 4.0)?;
    insert::param(&mut session, gate, EffectParam::Threshold, -50.0)?;
    insert::move_to(&mut session, comp, 0)?;                           // compressor first

    let master = session.memory.master().id();                         // the master is a track
    let limiter = insert::add(&mut session, master, Effect::Limiter)?;
    insert::param(&mut session, limiter, EffectParam::Ceiling, -1.0)?;

    transport::start(&mut session)?;
    let audio = session::steps::render_blocks(&mut session, 40)?;
    println!("peak {:.3}", audio.peak());
    println!("compressor is taking off {:.1} dB", insert::reduction(&session, comp).unwrap_or(0.0));

    insert::bypass(&mut session, gate, true)?;                         // state: saved, not an undo step
    insert::remove(&mut session, gate)?;
    session::undo(&mut session)?;                                      // back, parameters and all
    Ok(())
}
```

## Verbs

| verb | does | undo |
|---|---|---|
| `add(session, track, Effect) -> InsertId` | at the end of the chain, at the effect's defaults | one |
| `remove(session, insert)` | | one |
| `move_to(session, insert, index)` | `0` is first | one |
| `param(session, insert, EffectParam, value)` | heard at once | drag |
| `bypass(session, insert, on)` | the mixer is rebuilt without it | state |
| `reduction(session, insert) -> Option<f32>` | dB being taken off, for a meter | |

Every signature, argument, error and undo step: [insert reference](insert.ref.md).

## Effects

| `Effect` | parameters (`EffectParam`) | starts at |
|---|---|---|
| `Compressor` | `Threshold` dB, `Ratio`, `Attack` ms, `Release` ms, `Knee` dB, `Makeup` dB | -18 dB, 4:1, 10 ms, 100 ms, 6 dB, 0 dB |
| `Limiter` | `Ceiling` dBTP, `Release` ms | -1 dBTP, 50 ms |
| `Gate` | `Threshold` dB, `Ratio`, `Attack` ms, `Hold` ms, `Release` ms, `Range` dB | -40 dB, closed fully, 1 ms, 50 ms, 100 ms, 80 dB |
| `DeEsser` | `Frequency` Hz, `Threshold` dB, `Ratio` | 6 kHz, -20 dB, 4:1 |
| `Leveler` | `Target` dB, `Range` dB, `Speed` dB/s, `Floor` dB | toward -18 dB |
| `Plugin(PluginRef)` | its own: see [plugin](plugin.md) | where its saved state left it |

## Notes

* Any track takes inserts: audio, MIDI, a bus, the master. A pre-fader
  send taps after the last insert.
* A parameter the effect does not have is refused. Values the effect
  cannot use are clamped by the effect.
* The limiter looks ahead 5 ms; the mixer delays the other strips to match.
* `reduction` is `None` for an insert that is not in the mixer: bypassed,
  a plugin, or added since the last rebuild.
* A plugin insert (`Effect::Plugin`) is added, moved, bypassed and removed
  here like the others; its parameters are in the [plugin](plugin.md)
  flow. Bypassing a plugin does not rebuild the mixer: it stays loaded.
