//! Lists MIDI inputs and returns if there are none. Otherwise it listens
//! for 10 seconds on the port index from the first argument, or port 0,
//! and prints how many messages a full 1024-deep queue dropped.
//!
//! ```text
//! cargo run --example midi_monitor --features midi-io -- [port_index]
//! ```

use std::time::{Duration, Instant};

use superb::primitives::midi::input::{list_input_ports, MidiInputBridge};
use superb::primitives::midi::TimestampedMessage;
use superb::primitives::Result;

fn main() -> Result<()> {
    // Port names, then stop when the list is empty.
    let ports = list_input_ports()?;
    if ports.is_empty() {
        println!("no MIDI input ports found");
        return Ok(());
    }
    for p in &ports {
        println!("[{}] {}", p.index, p.name);
    }

    // First argument as a port index, otherwise 0.
    let index = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if std::env::args().nth(1).is_none() {
        assert_eq!(index, 0);
    }

    // The callback pushes into a queue of 1024. This thread drains it
    // for 10 seconds, sleeping 5 ms whenever the queue is empty.
    let (bridge, mut rx) = MidiInputBridge::connect(index, 1024)?;
    println!("listening on '{}' for 10 s ...", bridge.port_name());

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        while let Some(TimestampedMessage {
            timestamp_us,
            message,
        }) = rx.pop()
        {
            println!("{timestamp_us:>12} us  {message:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("dropped (queue full): {}", bridge.dropped());
    Ok(())
}
