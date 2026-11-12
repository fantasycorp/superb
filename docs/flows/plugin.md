# plugin

Find the plugins on this machine, and work the ones on your tracks. CLAP today.

## Example

```rust,no_run
use superb::primitives::plugins::PluginChange;
use superb::primitives::session::{Effect, PluginRef, TrackKind};
use superb::primitives::{Result, SampleRate};
use superb::{insert, plugin, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("plugins"), rate, 256)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;

    let found = plugin::scan();                                     // every CLAP plugin installed
    let Some(reverb) = found.iter().find(|p| p.is_audio_effect()) else {
        println!("no CLAP effect is installed");
        return Ok(());
    };
    println!("{} by {}", reverb.name, reverb.vendor);

    // A plugin is an insert. It is loaded here, once.
    let verb = insert::add(&mut session, vox, Effect::Plugin(PluginRef::from(reverb)))?;
    for p in plugin::params(&mut session, verb)? {
        println!("  {:>4}  {}  {}..{} (default {})", p.id, p.name, p.min, p.max, p.default);
    }
    if let Some(first) = plugin::params(&mut session, verb)?.first().cloned() {
        plugin::param(&mut session, verb, first.id, first.max)?;    // heard at once; one undo step per drag
        println!("{} is now {}", first.name, plugin::value(&mut session, verb, first.id)?);
    }

    transport::start(&mut session)?;
    // While a control for the plugin is on screen: what it changed itself.
    for change in plugin::changes(&mut session, verb)? {
        match change {
            PluginChange::Touched { id } => println!("{id} is held in the plugin's own view"),
            PluginChange::Value { id, value } => println!("{id} moved to {value}"),
            PluginChange::Released { id } => println!("{id} was let go"),
        }
    }
    track::add(&mut session, "gtr", TrackKind::Audio)?;             // rebuilds the mixer; the plugin keeps running
    insert::bypass(&mut session, verb, true)?;                      // state; the plugin stays loaded
    session::save(&mut session)?;                                   // its state goes to plugins/<insert id>.state
    insert::remove(&mut session, verb)?;
    session::undo(&mut session)?;                                   // loaded again, as it was
    Ok(())
}
```

## Verbs

| verb | does | undo |
|---|---|---|
| `scan() -> Vec<PluginDescriptor>` | every CLAP plugin installed, and those under `CLAP_PATH` | |
| `insert::add(session, track, Effect::Plugin(PluginRef::from(&descriptor)))` | loads the plugin and puts it on the track | one |
| `params(session, insert) -> Vec<ParamInfo>` | id, name, range and default of each parameter | |
| `param(session, insert, id, value)` | heard at the plugin's next block | drag |
| `value(session, insert, id) -> f64` | read from the plugin, now | |
| `changes(session, insert) -> Vec<PluginChange>` | what the plugin changed itself since last asked: `Touched`, `Value`, `Released` | drag, per value |
| `missing(session) -> Vec<(InsertId, SuperbError)>` | the plugin inserts that could not be loaded, and why | |
| `insert::remove`, `move_to`, `bypass` | as for any insert | one, one, state |

Every signature, argument, error and undo step: [plugin reference](plugin.ref.md).

## Notes

* A plugin is loaded once, when its insert is added or at the first
  rebuild after `session::open`. Every later rebuild of the mixer hands
  the running instance to the new mixer between two blocks, so its state
  and its tails carry on and nothing is loaded again.
* `session::save` asks each plugin for its state and writes it to a file
  of its own, `plugins/<insert id>.state` (`Layout::plugin_state`), not
  into `memory.json`: the bytes are the plugin's, and can be large.
  `insert::remove` and archiving a track take the state too before the
  plugin is let go, so undo and `track::restore` bring it back as it was.
  The file of an insert that is gone is deleted at the next save.
* A control drawn for a plugin has two directions. `param` sets, and
  `changes` says what the plugin did itself: a knob turned in its own
  view. Ask for `changes` on the UI's timer. A value
  it reports is taken into the memory as `param` would put it, so it is
  saved, and undo sends the plugin back to the value before.
* The state file holds the plugin at one moment. Parameter values in the
  memory are applied on top of it when the plugin is loaded, and
  automation, when it is built, is applied on top of both. A change can
  already be placed on a frame inside a block (`ParamChange::at`), which
  is what automation will use.
* A plugin that cannot be loaded when a session is opened does not stop
  the session: its insert is kept, saved as it was, left out of the mixer
  and listed by `missing`. `insert::add` of a plugin that cannot be
  loaded is refused.
* `export::bounce` and `export::stems` run a second instance of each
  plugin, brought to the state the running one has, so a bounce does not
  disturb what is playing.
* A plugin needs as many input and output channels as the session
  (stereo). An instrument, which has no audio input, is refused for now.
* To host another format, or load from somewhere else, replace
  `session.plugin_loader`: a function from a `PluginRef` to a
  `Box<dyn PluginInstance>`.
