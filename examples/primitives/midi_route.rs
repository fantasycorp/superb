//! Note 69, 440 Hz, goes only to synth A. It starts at frame 240 of a 480-frame
//! block and stops at frame 119 of the next one. Synth B stays at 0.
//!
//! ```text
//! cargo run --example midi_route
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::graph::nodes::{MonoSynthNode, PassThroughNode};
use superb::primitives::graph::{AudioGraph, MidiPort};
use superb::primitives::io::Engine;
use superb::primitives::midi::timing::us_to_frames;
use superb::primitives::midi::{MidiChannel, MidiMessage, TimestampedMessage};
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate, SuperbError};

fn main() -> Result<()> {
    let sample_rate = SampleRate::HZ_48K;
    // 480 frames at 48 kHz is 10 ms.
    let block = 480;
    assert_eq!(sample_rate.seconds_to_samples(0.01), block as i64);

    let mut graph = AudioGraph::new();
    // Both synths sum into the sink. The only MIDI edge is external → A,
    // so B does not hear the note.
    let synth_a = graph.add_node(Box::new(MonoSynthNode::new(1)));
    let synth_b = graph.add_node(Box::new(MonoSynthNode::new(1)));
    let sink = graph.add_node(Box::new(PassThroughNode::new(1)));
    graph.connect(synth_a, sink)?;
    graph.connect(synth_b, sink)?;
    graph.set_output(sink)?;
    graph.connect_midi(MidiPort::External, synth_a)?;

    let mut engine = Engine::new(graph, Transport::new(sample_rate, 120.0));
    engine.prepare(sample_rate, block)?;
    engine.transport_mut().play();

    // Note 69 is 440 Hz. Velocity 127 is amplitude 1. Stamped 5 ms ahead,
    // which is frame 240. The stream clock is still 0.
    let (mut tx, rx) = superb::primitives::realtime::spsc::channel::<TimestampedMessage>(16);
    engine.set_midi_input(Some(rx));
    let channel = MidiChannel::new(0).unwrap();
    let note = 69;
    assert!((MonoSynthNode::note_to_hz(note) - 440.0).abs() < 1e-9);
    assert_eq!(engine.stream_clock_us(), 0);
    assert_eq!(us_to_frames(5_000, sample_rate), 240);
    let note_on_us = engine.stream_clock_us() + 5_000;
    assert_eq!(note_on_us, 5_000);
    tx.push(TimestampedMessage {
        timestamp_us: note_on_us,
        message: MidiMessage::NoteOn {
            channel,
            note,
            velocity: 127,
        },
    })
    .map_err(|_| SuperbError::Message("midi queue full".into()))?;

    let mut out = PlanarBuffer::new(1, block);
    engine.render(&mut out)?;

    // Frames 0..240 are silent and 240..480 sound. A peaks at 1.000, B at 0,
    // and the mix matches A.
    let silent_before = out.channel(0)[..240].iter().all(|&s| s == 0.0);
    let sounding_after = out.channel(0)[240..].iter().any(|&s| s != 0.0);
    assert!(silent_before);
    assert!(sounding_after);
    println!(
        "block 0: note-on stamped {note_on_us} us -> frame 240: silent before = {silent_before}, sounding after = {sounding_after}"
    );
    let (peak_a, peak_b) = {
        let graph = engine.processor();
        (
            graph.node_output(synth_a)?.peak(),
            graph.node_output(synth_b)?.peak(),
        )
    };
    let peak_mix = out.peak();
    assert_eq!(format!("{peak_a:.3}"), "1.000");
    assert_eq!(peak_b.to_bits(), 0);
    assert_eq!(format!("{peak_mix:.3}"), "1.000");
    println!(
        "  synth A peak {:.3}, synth B peak {:.3}, mix peak {:.3}",
        peak_a, peak_b, peak_mix
    );

    // The first block advanced the clock 10 ms. Note-off 2.5 ms later is
    // frame 120, so the last sounding sample is 119.
    assert_eq!(engine.stream_clock_us(), 10_000);
    assert_eq!(us_to_frames(2_500, sample_rate), 120);
    let note_off_us = engine.stream_clock_us() + 2_500;
    assert_eq!(note_off_us, 12_500);
    tx.push(TimestampedMessage {
        timestamp_us: note_off_us,
        message: MidiMessage::NoteOff {
            channel,
            note,
            velocity: 0,
        },
    })
    .map_err(|_| SuperbError::Message("midi queue full".into()))?;
    engine.render(&mut out)?;
    let last_sound = out.channel(0).iter().rposition(|&s| s != 0.0);
    assert_eq!(last_sound, Some(119));
    println!("block 1: note-off stamped {note_off_us} us -> last non-zero sample at frame {last_sound:?} (expected Some(119))");
    let stats = engine.stats();
    assert_eq!(stats.blocks, 2);
    assert_eq!(stats.segments, 2);
    assert_eq!(stats.midi_dropped, 0);
    assert_eq!(stats.capture_dropped, 0);
    println!("engine stats: {:?}", engine.stats());
    Ok(())
}
