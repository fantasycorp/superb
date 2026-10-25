//! Three graphs swap in between blocks: 440 Hz through a gain of 0.5,
//! then 440 Hz and 660 Hz through a gain of 0.125, then a constant 0.75.
//! The retired graph is dropped back on this thread.
//!
//! ```text
//! cargo run --example live_graph_edit
//! ```

use superb::primitives::graph::nodes::{ConstantNode, GainNode, PassThroughNode, SineNode};
use superb::primitives::graph::{live_pair, BlockTime, LiveGraph};
use superb::primitives::midi::MidiEventBuffer;
use superb::primitives::{Result, SampleRate};

fn main() -> Result<()> {
    let sample_rate = SampleRate::HZ_48K;
    assert_eq!(sample_rate.seconds_to_samples(1.0), 48_000);
    // Each process call is 256 frames. No MIDI in this render.
    let block = 256;
    let midi = MidiEventBuffer::with_capacity(0);

    let (mut editor, mut live) = live_pair();
    editor.configure(sample_rate, block);

    // Generation 1: 440 Hz at amplitude 1, stereo, into a gain of 0.5.
    let sine_node = SineNode::new(440.0, 1.0, 2);
    assert!((sine_node.frequency().get() - 440.0).abs() < 1e-6);
    assert!((sine_node.amplitude().get() - 1.0).abs() < 1e-6);
    let sine = editor.graph_mut().add_node(Box::new(sine_node));
    let gain_node = GainNode::new(0.5, 2);
    assert!((gain_node.gain().get() - 0.5).abs() < 1e-6);
    let gain = editor.graph_mut().add_node(Box::new(gain_node));
    editor.graph_mut().connect(sine, gain)?;
    editor.graph_mut().set_output(gain)?;
    editor.publish()?;
    println!("published generation 1 (sine -> gain 0.5)");

    let mut time = BlockTime {
        rolling: true,
        ..BlockTime::free_running()
    };

    let mut render = |live: &mut LiveGraph, label: &str, blocks: usize| -> Result<f32> {
        let mut peak = 0.0f32;
        for _ in 0..blocks {
            live.process(time, block, &midi, None)?;
            time = time.advanced_by(block, sample_rate);
            peak = peak.max(live.output().peak());
        }
        println!(
            "{label}: {blocks} blocks, peak {peak:.3}, active nodes {}, swaps {}",
            live.active_node_count(),
            live.swap_count()
        );
        Ok(peak)
    };

    // 20 blocks. Peak 0.500, 2 nodes, 1 swap.
    let peak = render(&mut live, "gen 1 running   ", 20)?;
    assert_eq!(format!("{peak:.3}"), "0.500");
    assert_eq!(live.active_node_count(), 2);
    assert_eq!(live.swap_count(), 1);

    // Generation 2 is drafted while generation 1 keeps running: 440 Hz and
    // 660 Hz, summed, through a gain of 0.125, then a pass-through.
    let sine_a_node = SineNode::new(440.0, 1.0, 2);
    let sine_b_node = SineNode::new(660.0, 1.0, 2);
    assert!((sine_a_node.frequency().get() - 440.0).abs() < 1e-6);
    assert!((sine_b_node.frequency().get() - 660.0).abs() < 1e-6);
    let sine_a = editor.graph_mut().add_node(Box::new(sine_a_node));
    let sine_b = editor.graph_mut().add_node(Box::new(sine_b_node));
    let gain_node = GainNode::new(0.125, 2);
    assert!((gain_node.gain().get() - 0.125).abs() < 1e-6);
    let gain = editor.graph_mut().add_node(Box::new(gain_node));
    let bus = editor
        .graph_mut()
        .add_node(Box::new(PassThroughNode::new(2)));
    editor.graph_mut().connect(sine_a, gain)?;
    editor.graph_mut().connect(sine_b, gain)?;
    editor.graph_mut().connect(gain, bus)?;
    editor.graph_mut().set_output(bus)?;
    // Still generation 1: 10 blocks, peak 0.500, 2 nodes, 1 swap.
    let peak = render(&mut live, "gen 1 while edit", 10)?;
    assert_eq!(format!("{peak:.3}"), "0.500");
    assert_eq!(live.active_node_count(), 2);
    assert_eq!(live.swap_count(), 1);
    editor.publish()?;
    // Published and not yet processed, so one graph is pending.
    assert_eq!(editor.pending_count(), 1);
    println!(
        "published generation 2 (two sines -> gain 0.125 -> bus); pending: {}",
        editor.pending_count()
    );

    // 20 blocks. The summed sines through 0.125 peak at 0.238, under the
    // 0.250 of a perfect stack. 4 nodes, 2 swaps.
    let peak = render(&mut live, "gen 2 running   ", 20)?;
    assert_eq!(format!("{peak:.3}"), "0.238");
    assert!(peak < 0.25);
    assert_eq!(live.active_node_count(), 4);
    assert_eq!(live.swap_count(), 2);

    // Generation 3: DC 0.75 on both channels.
    let constant = ConstantNode::new(0.75, 2);
    assert!((constant.value().get() - 0.75).abs() < 1e-6);
    let c = editor.graph_mut().add_node(Box::new(constant));
    editor.graph_mut().set_output(c)?;
    editor.publish()?;
    println!("published generation 3 (constant 0.75)");
    // 10 blocks. Peak 0.750, 1 node, 3 swaps.
    let peak = render(&mut live, "gen 3 running   ", 10)?;
    assert_eq!(format!("{peak:.3}"), "0.750");
    assert!((peak - 0.75).abs() < 1e-6);
    assert_eq!(live.active_node_count(), 1);
    assert_eq!(live.swap_count(), 3);

    // publish already dropped the earlier retired graphs. Generation 2
    // is the one left, so this reclaims 1. Applied 3, rejected 0.
    let reclaimed = editor.try_reclaim();
    assert_eq!(reclaimed, 1);
    assert_eq!(editor.applied_count(), 3);
    assert_eq!(editor.rejected_count(), 0);
    println!(
        "reclaimed {reclaimed} retired graph(s) on the control thread; applied {}, rejected {}",
        editor.applied_count(),
        editor.rejected_count()
    );
    Ok(())
}
