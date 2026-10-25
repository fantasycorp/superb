//! Plays a 440 Hz sine at amplitude 0.2 through a unity gain for 3 seconds
//! on the default output, then prints callback count, load and overruns.
//! With no output device, opening the stream returns an error.
//!
//! ```text
//! cargo run --example play_sine_device --features devices
//! ```

use std::time::Duration;

use superb::primitives::graph::nodes::{GainNode, SineNode};
use superb::primitives::graph::AudioGraph;
use superb::primitives::io::device::{output_devices, run_engine, OutputConfig};
use superb::primitives::io::Engine;
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate};

fn main() -> Result<()> {
    // Every output device. The default is marked.
    for d in output_devices()? {
        println!("{}{}", d.name, if d.is_default { " (default)" } else { "" });
    }

    // Stereo 440 Hz at amplitude 0.2, then a gain of 1.
    let mut graph = AudioGraph::new();
    let sine = SineNode::new(440.0, 0.2, 2);
    assert!((sine.frequency().get() - 440.0).abs() < 1e-6);
    assert!((sine.amplitude().get() - 0.2).abs() < 1e-6);
    let gain_node = GainNode::new(1.0, 2);
    assert!((gain_node.gain().get() - 1.0).abs() < 1e-6);
    let osc = graph.add_node(Box::new(sine));
    let gain = graph.add_node(Box::new(gain_node));
    graph.connect(osc, gain)?;
    graph.set_output(gain)?;

    let mut engine = Engine::new(graph, Transport::new(SampleRate::HZ_48K, 120.0));
    engine.transport_mut().play();
    let load = engine.load();

    // Default device. The callback updates `load` while this thread sleeps
    // 3 seconds. Dropping the stream stops playback. The counts below depend
    // on the device, so they are not asserted.
    let config = OutputConfig::default();
    let stream = run_engine(&config, engine)?;
    println!(
        "playing on '{}' @ {} ...",
        stream.device_name(),
        stream.sample_rate()
    );
    std::thread::sleep(Duration::from_secs(3));
    println!(
        "callbacks: {}, avg load: {:.1}%, peak load: {:.1}%, overruns: {}",
        load.callbacks(),
        load.load() * 100.0,
        load.peak() * 100.0,
        load.overruns()
    );
    Ok(())
}
