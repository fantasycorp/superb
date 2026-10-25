//! Three seconds of 440 Hz through a gain of 0.5 and a strip at −6 dB,
//! panned to −0.3. The file is 144000 stereo frames, peaks 0.171 and
//! 0.105. The 3-beat loop makes 563 blocks into 564 segments.
//!
//! ```text
//! cargo run --example sine_through_graph
//! ```

use superb::primitives::buffers::gain::db_to_linear;
use superb::primitives::graph::nodes::{GainNode, SineNode};
use superb::primitives::io::offline;
use superb::primitives::io::wav::{self, WavFormat};
use superb::primitives::io::Engine;
use superb::primitives::mixer::{Mixer, PanLaw};
use superb::primitives::transport::{LoopRange, Transport};
use superb::primitives::{BeatPosition, Result, SampleRate, TimePosition};

fn main() -> Result<()> {
    let sample_rate = SampleRate::HZ_48K;
    assert_eq!(sample_rate.seconds_to_samples(1.0), 48_000);
    let block = 256;
    let seconds = 3.0;

    // Stereo sine at 440 Hz, amplitude 0.8, then a gain of 0.5. The track
    // fader is −6 dB and constant-power pan is −0.3. The master is balance
    // at centre, gain 1, so it does not change the level.
    let mut mixer = Mixer::new(2);
    let track = mixer.add_track("lead");
    let osc_node = SineNode::new(440.0, 0.8, 2);
    assert!((osc_node.frequency().get() - 440.0).abs() < 1e-6);
    assert!((osc_node.amplitude().get() - 0.8).abs() < 1e-6);
    let osc = mixer.graph_mut().add_node(Box::new(osc_node));
    let gain_node = GainNode::new(0.5, 2);
    assert!((gain_node.gain().get() - 0.5).abs() < 1e-6);
    let gain = mixer.graph_mut().add_node(Box::new(gain_node));
    mixer.graph_mut().connect(osc, gain)?;
    mixer.connect_source(gain, track)?;
    let params = mixer.track_params(track)?;
    params.set_volume_db(-6.0);
    params.set_pan(-0.3);
    assert!((params.volume_db() + 6.0).abs() < 1e-3);
    assert!((params.pan() + 0.3).abs() < 1e-6);
    assert_eq!(params.pan_law(), PanLaw::ConstantPower);
    let master = mixer.bus_params(mixer.master())?;
    assert_eq!(master.pan_law(), PanLaw::Balance);
    assert!(master.pan().abs() < 1e-6);
    assert!((master.gain() - 1.0).abs() < 1e-6);

    // A 3-beat loop at 120 bpm is 72000 samples. Three seconds is 144000
    // frames, exactly two trips around that loop.
    let mut transport = Transport::new(sample_rate, 120.0);
    let loop_end = transport
        .tempo_map()
        .beat_to_samples(BeatPosition::new(3.0));
    assert_eq!(loop_end.samples(), 72_000);
    transport.set_loop(Some(LoopRange::new(TimePosition::ZERO, loop_end)?));
    transport.set_loop_enabled(true);
    transport.play();

    let mut engine = Engine::new(mixer, transport);
    let total = sample_rate.seconds_to_samples(seconds) as usize;
    assert_eq!(total, 144_000);
    let audio = offline::render(&mut engine, sample_rate, block, total)?;
    assert_eq!(audio.frames(), 144_000);
    assert_eq!(audio.channels(), 2);

    let path = "sine_through_graph.wav";
    wav::write(path, &audio, sample_rate, WavFormat::Float32)?;

    // 0.8 × 0.5 × −6 dB × constant-power pan −0.3. Left is the louder side.
    // The sine reaches full scale, so the file peaks at 0.171 and 0.105.
    let (pan_l, pan_r) = PanLaw::ConstantPower.gains(-0.3);
    let scale = 0.8 * 0.5 * db_to_linear(-6.0);
    let peak_l = audio.channel(0).iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let peak_r = audio.channel(1).iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak_l - scale * pan_l).abs() < 1e-3);
    assert!((peak_r - scale * pan_r).abs() < 1e-3);
    assert!(peak_l > peak_r);
    assert_eq!(format!("{peak_l:.3}"), "0.171");
    assert_eq!(format!("{peak_r:.3}"), "0.105");
    assert_eq!(format!("{:.3}", params.peak(0)), "0.171");
    assert_eq!(format!("{:.3}", params.peak(1)), "0.105");

    // 144000 / 256 is 562 blocks of 256 plus a tail of 128, so 563 blocks.
    // One of them crosses the loop and splits, so 564 segments. The playhead
    // lands on the loop end and wraps to 0, bar 1 beat 1.
    let stats = engine.stats();
    assert_eq!(144_000 / block, 562);
    assert_eq!(144_000 % block, 128);
    assert_eq!(stats.blocks, 563);
    assert_eq!(stats.segments, 564);
    let snap = engine.transport().snapshot();
    assert_eq!(snap.position.samples(), 0);
    assert_eq!(snap.bbt.to_string(), "1.1.000");

    println!(
        "wrote {path}: {} frames x {} ch @ {}",
        audio.frames(),
        audio.channels(),
        sample_rate
    );
    println!("peak L/R: {:.3} / {:.3}", peak_l, peak_r);
    println!(
        "blocks: {}, segments (loop wraps add one): {}",
        stats.blocks, stats.segments
    );
    println!("transport now at {} = {}", snap.position, snap.bbt);
    println!(
        "track meter L/R: {:.3} / {:.3}",
        params.peak(0),
        params.peak(1)
    );
    Ok(())
}
