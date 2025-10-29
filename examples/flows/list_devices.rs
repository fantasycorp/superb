//! `devices` lists the machine's audio and MIDI ports.
//!
//! ```text
//! devices::outputs / inputs
//! devices::midi_inputs   (midi-io)
//! ```
//!
//! ```text
//! for d in devices::outputs()? {
//!     println!("{}{}", d.name, if d.is_default { " *" } else { "" });
//! }
//! ```
//!
//! ```text
//! cargo run --example list_devices --features devices
//! cargo run --example list_devices --features hardware   # audio + MIDI
//! ```

use superb::devices;
use superb::primitives::Result;

/// `0` prints "channels unknown".
fn channels(max: usize) -> String {
    if max == 0 {
        "channels unknown".into()
    } else {
        format!("{max} ch max")
    }
}

fn main() -> Result<()> {
    // Default host outputs. * is the default (what None selects).
    println!("outputs:");
    for d in devices::outputs()? {
        println!(
            "  {}{} ({})",
            d.name,
            if d.is_default { " *" } else { "" },
            channels(d.max_channels)
        );
    }
    // Default host inputs.
    println!("inputs:");
    for d in devices::inputs()? {
        println!(
            "  {}{} ({})",
            d.name,
            if d.is_default { " *" } else { "" },
            channels(d.max_channels)
        );
    }

    // MIDI inputs (midi-io).
    #[cfg(feature = "midi-io")]
    {
        println!("MIDI inputs:");
        for p in devices::midi_inputs()? {
            println!("  {}: {}", p.index, p.name);
        }
    }
    Ok(())
}
