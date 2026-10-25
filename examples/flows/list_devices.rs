//! `devices`: every output, every input, and with `midi-io` every MIDI port.
//!
//! Each comment is the block under it. The names are the machine's.
//!
//! ```text
//! cargo run --example list_devices --features devices
//! cargo run --example list_devices --features hardware   # audio + MIDI
//! ```

use superb::devices;
use superb::primitives::Result;

/// `0` is an unknown channel count. The listing prints "channels unknown".
fn channels(max: usize) -> String {
    if max == 0 {
        "channels unknown".into()
    } else {
        format!("{max} ch max")
    }
}

fn main() -> Result<()> {
    // `0` prints "channels unknown". Any other count prints "N ch max".
    // Output listings always carry 0: `output_devices` leaves
    // `max_channels` at 0. An input listing carries 0 when it has no
    // supported config to measure.
    assert_eq!(channels(0), "channels unknown");
    assert_eq!(channels(2), "2 ch max");

    // `devices::outputs` is the default host's outputs. `*` marks
    // `is_default`, the device a later `None` selects. Every line says
    // the channel count is unknown.
    println!("outputs:");
    let outputs = devices::outputs()?;
    assert!(outputs.iter().all(|d| d.max_channels == 0));
    for d in &outputs {
        println!(
            "  {}{} ({})",
            d.name,
            if d.is_default { " *" } else { "" },
            channels(d.max_channels)
        );
    }
    // `devices::inputs` is the default host's inputs, with `*` on the
    // default. `max_channels` is the largest supported config.
    println!("inputs:");
    for d in devices::inputs()? {
        println!(
            "  {}{} ({})",
            d.name,
            if d.is_default { " *" } else { "" },
            channels(d.max_channels)
        );
    }

    // `hardware` includes `midi-io`, which adds this list: each port's
    // index and name from `devices::midi_inputs`. `devices` alone skips it.
    #[cfg(feature = "midi-io")]
    {
        println!("MIDI inputs:");
        for p in devices::midi_inputs()? {
            println!("  {}: {}", p.index, p.name);
        }
    }
    Ok(())
}
