//! Two seconds of 330 Hz at amplitude 0.6, fader −3 dB, constant-power
//! centre. The bounce peaks at 0.300: 96000 frames at 48 kHz, then
//! 88200 frames at 44.1 kHz.
//!
//! ```text
//! cargo run --example decode_bounce
//! ```

use superb::primitives::buffers::gain::db_to_linear;
use superb::primitives::graph::nodes::SineNode;
use superb::primitives::io::files::{bounce_wav, decode, resampled_frames};
use superb::primitives::io::wav::WavFormat;
use superb::primitives::io::Engine;
use superb::primitives::mixer::{Mixer, PanLaw};
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    assert_eq!(rate.seconds_to_samples(1.0), 48_000);

    // Stereo 330 Hz at amplitude 0.6 into a track at −3 dB. Centre pan
    // uses constant power, so each side is multiplied by √2/2. The master
    // is balance at centre and does not change the level.
    let mut mixer = Mixer::new(2);
    let track = mixer.add_track("tone");
    let osc_node = SineNode::new(330.0, 0.6, 2);
    assert!((osc_node.frequency().get() - 330.0).abs() < 1e-6);
    assert!((osc_node.amplitude().get() - 0.6).abs() < 1e-6);
    let osc = mixer.graph_mut().add_node(Box::new(osc_node));
    mixer.connect_source(osc, track)?;
    let params = mixer.track_params(track)?;
    params.set_volume_db(-3.0);
    assert!((params.volume_db() + 3.0).abs() < 1e-3);
    assert!(params.pan().abs() < 1e-6);
    assert_eq!(params.pan_law(), PanLaw::ConstantPower);
    let master = mixer.bus_params(mixer.master())?;
    assert_eq!(master.pan_law(), PanLaw::Balance);
    assert!((master.gain() - 1.0).abs() < 1e-6);

    let mut engine = Engine::new(mixer, Transport::new(rate, 120.0));
    engine.transport_mut().play();

    // Two seconds is 96000 frames. The sine reaches full scale, so the
    // peak is 0.6 × −3 dB × √2/2, which prints as 0.300.
    let bounced = "decode_bounce_48k.wav";
    let frames = rate.hz() as usize * 2;
    assert_eq!(frames, 96_000);
    let rendered = bounce_wav(&mut engine, rate, 256, frames, bounced, WavFormat::Float32)?;
    let (side, _) = PanLaw::ConstantPower.gains(0.0);
    let expected = 0.6 * db_to_linear(-3.0) * side;
    assert_eq!(rendered.frames(), 96_000);
    assert_eq!(rendered.channels(), 2);
    assert!((rendered.peak() - expected).abs() < 1e-3);
    assert_eq!(format!("{:.3}", rendered.peak()), "0.300");
    println!(
        "bounced  {bounced}: {} frames, {} ch, peak {:.3}",
        rendered.frames(),
        rendered.channels(),
        rendered.peak()
    );

    // Reading the file back gives the same 96000 frames, 2.00 s, peak 0.300.
    let file = decode(bounced)?;
    assert_eq!(file.frames(), 96_000);
    assert_eq!(file.sample_rate.hz(), 48_000);
    assert_eq!(format!("{:.2}", file.duration_seconds()), "2.00");
    assert_eq!(format!("{:.3}", file.buffer.peak()), "0.300");
    println!(
        "decoded  {bounced}: {} frames @ {} ({:.2} s), peak {:.3}",
        file.frames(),
        file.sample_rate,
        file.duration_seconds(),
        file.buffer.peak()
    );

    // 96000 × 44100 / 48000 = 88200 frames. The float buffer still peaks
    // at 0.300; the file is written as 16-bit PCM.
    let target = SampleRate::CD;
    assert_eq!(target.hz(), 44_100);
    let converted = file.resampled_to(target)?;
    assert_eq!(resampled_frames(96_000, rate, target), 88_200);
    assert_eq!(converted.frames(), 88_200);
    assert_eq!(
        converted.frames(),
        resampled_frames(file.frames(), file.sample_rate, target)
    );
    assert_eq!(format!("{:.3}", converted.buffer.peak()), "0.300");
    let out = "decode_bounce_44k1.wav";
    converted.write_wav(out, WavFormat::Pcm16)?;
    println!(
        "exported {out}: {} frames @ {}, peak {:.3}",
        converted.frames(),
        converted.sample_rate,
        converted.buffer.peak()
    );

    // The 16-bit file reads back as 88200 frames at 44100 Hz.
    let check = decode(out)?;
    assert_eq!(check.frames(), 88_200);
    assert_eq!(check.sample_rate.hz(), 44_100);
    println!(
        "re-read  {out}: {} frames @ {}",
        check.frames(),
        check.sample_rate
    );
    Ok(())
}
