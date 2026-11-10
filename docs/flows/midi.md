# midi

Play notes from code; connect a keyboard.

## Example

```rust
use superb::primitives::midi::{MidiChannel, MidiMessage};
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate};
use superb::{midi, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("keys"), rate, 128)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;
    track::monitor(&mut session, keys, true)?;          // synth on the strip so notes are audible
    transport::start(&mut session)?;

    midi::play_note(&mut session, 0, 60, 100)?;         // channel 0, middle C, velocity 100
    for note in [64, 67] {
        midi::play_note(&mut session, 0, note, 90)?;
    }
    let audio = session::steps::render_blocks(&mut session, 8)?;
    println!("C major, peak {:.3}", audio.peak());

    for note in [60, 64, 67] {
        midi::release_note(&mut session, 0, note)?;
    }
    midi::send(&mut session, MidiMessage::control_change(MidiChannel::new(0)?, 1, 64)?)?;   // any message
    Ok(())
}
```

A hardware keyboard, with the `hardware` feature:

```rust,no_run
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate};
use superb::{devices, midi, session, track, transport};

fn main() -> Result<()> {
    let mut session = session::create(std::env::temp_dir().join("keyboard"), SampleRate::HZ_48K, 128)?;
    let keys = track::add(&mut session, "keys", TrackKind::Midi)?;
    track::monitor(&mut session, keys, true)?;

    for port in devices::midi_inputs()? {
        println!("{}: {}", port.index, port.name);
    }
    midi::connect_input(&mut session, 0)?;      // its notes play on every monitoring MIDI track
    devices::output(&mut session, None)?;       // the speakers drive the engine
    transport::start(&mut session)?;
    std::thread::sleep(std::time::Duration::from_secs(10));   // play
    Ok(())
}
```

`cargo run --example midi_keys` runs both.

## Verbs

| verb | does |
|---|---|
| `play_note(session, channel, note, velocity)` | queue a note-on for the next block |
| `release_note(session, channel, note)` | |
| `send(session, MidiMessage)` | queue any message |
| `connect_input(session, port_index)` | route a port into the engine (feature `midi-io`) |

Every signature, argument, error and undo step: [midi reference](midi.ref.md).

## Notes

* MIDI sounds on every MIDI track that is monitoring, on the channel it
  listens to (`track::midi_channel`, all by default). While a recording
  runs it also lands as notes on every armed MIDI track.
* MIDI clips on the timeline play whether or not a track monitors.
* Hardware notes land at block resolution until the clocks are aligned
  (see ARCHITECTURE.md, "MIDI").
