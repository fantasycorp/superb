//! The UI pushes into an 8-deep queue and the audio thread pops. A push
//! past 8 is refused. Which block sees play, seek or stop follows the
//! sleeps, so those printed positions are not pinned down.
//!
//! ```text
//! cargo run --example realtime_queue
//! ```

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use superb::primitives::realtime::spsc;
use superb::primitives::realtime::AtomicF32;
use superb::primitives::Result;

/// What the UI sends to the audio thread. Plain data, no heap.
#[derive(Debug, Clone, Copy)]
enum Message {
    Play,
    Stop,
    Seek(i64),
}

fn main() -> Result<()> {
    // The queue holds 8 messages and does not grow.
    let (mut to_audio, mut from_ui) = spsc::channel::<Message>(8);
    assert_eq!(to_audio.capacity(), 8);
    // The fader starts at 1. The UI later stores 0.5.
    let fader = Arc::new(AtomicF32::new(1.0));
    assert!((fader.get() - 1.0).abs() < 1e-6);
    let fader_for_audio = Arc::clone(&fader);

    // Twenty blocks, 5 ms apart. Each playing block adds 256 frames.
    // Blocks 4, 9, 14 and 19 print whatever has arrived.
    let audio = thread::spawn(move || {
        let mut playing = false;
        let mut position = 0i64;
        for block in 0..20 {
            while let Some(msg) = from_ui.pop() {
                match msg {
                    Message::Play => playing = true,
                    Message::Stop => playing = false,
                    Message::Seek(to) => position = to,
                }
            }
            let gain = fader_for_audio.get();
            if playing {
                position += 256;
            }
            if block % 5 == 4 {
                println!("audio: block {block:>2}  playing={playing}  position={position:>5}  gain={gain:.2}");
            }
            thread::sleep(Duration::from_millis(5));
        }
        from_ui.is_abandoned()
    });

    to_audio.push(Message::Play).ok();
    thread::sleep(Duration::from_millis(30));
    fader.set(0.5);
    assert!((fader.get() - 0.5).abs() < 1e-6);
    to_audio.push(Message::Seek(48_000)).ok();
    thread::sleep(Duration::from_millis(30));
    to_audio.push(Message::Stop).ok();

    // Twelve Seek(0) pushes. The ones that do not fit are returned.
    let mut refused = 0;
    for _ in 0..12 {
        if to_audio.push(Message::Seek(0)).is_err() {
            refused += 1;
        }
    }
    println!(
        "ui: {refused} of 12 rapid messages refused (queue holds {})",
        to_audio.capacity()
    );

    // Dropping the sender is what makes the audio side see it as gone.
    drop(to_audio);
    let ui_gone_when_audio_finished = audio.join().expect("audio thread");
    println!("audio saw the UI side dropped: {ui_gone_when_audio_finished}");
    Ok(())
}
