//! `export`: the whole session as 16-bit PCM, then the overlap as float.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example export_session
//! cargo run --example export_session --features files
//! ```

use superb::export::{self, Bounce};
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::io::wav::{self, WavFormat};
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimeDuration, TimePosition};
use superb::{clip, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(
        std::env::temp_dir().join("export_session.superb"),
        rate,
        256,
    )?;
    let lead = track::add(&mut session, "lead", TrackKind::Audio)?;
    let pad = track::add(&mut session, "pad", TrackKind::Audio)?;

    // One second (48_000 frames) each. lead is a 440 Hz sine at 0.3 from
    // sample 0. pad is a 110 Hz sine at 0.2 from sample 24_000 (0.5 s).
    // The session ends at sample 72_000 (1.5 s). pad's fader is −6 dB.
    // lead's pan is 0.3, on the constant-power law a bounce builds with.
    let mut a = PlanarBuffer::new(2, rate.hz() as usize);
    let mut b = PlanarBuffer::new(2, rate.hz() as usize);
    for i in 0..a.frames() {
        let t = i as f32 / rate.hz() as f32;
        let sa = 0.3 * (t * 440.0 * core::f32::consts::TAU).sin();
        let sb = 0.2 * (t * 110.0 * core::f32::consts::TAU).sin();
        for ch in 0..2 {
            a.channel_mut(ch)[i] = sa;
            b.channel_mut(ch)[i] = sb;
        }
    }
    session.audio.insert("a.wav", a);
    session.audio.insert("b.wav", b);
    clip::add(&mut session, lead, "a.wav", TimePosition::ZERO)?;
    clip::add(
        &mut session,
        pad,
        "b.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;
    track::volume(&mut session, pad, -6.0)?;
    track::pan(&mut session, lead, 0.3)?;
    assert_eq!(
        session.memory.track(lead)?.clips()[0].length().samples(),
        48_000
    );
    assert_eq!(
        session.memory.track(pad)?.clips()[0].start().samples(),
        24_000
    );
    assert_eq!(
        session.memory.track(pad)?.clips()[0].length().samples(),
        48_000
    );
    assert!((session.memory.track(pad)?.state.volume_db + 6.0).abs() < 1e-6);
    assert!((session.memory.track(lead)?.state.pan - 0.3).abs() < 1e-6);

    // `Bounce::default` is 16-bit PCM from sample 0 to the end of the last
    // clip: 72_000 frames, 1.50 s. The two sines meet from 0.5 s to 1.0 s
    // and that overlap peaks at 0.321. The live playhead stays at sample 0.
    let dir = std::env::temp_dir();
    let path = dir.join("export_session.wav");
    let audio = export::bounce(&mut session, &path, Bounce::default())?;
    assert_eq!(audio.frames(), 72_000);
    assert!((audio.peak() - 0.321).abs() < 1e-3);
    assert_eq!(transport::playhead(&session).samples(), 0);
    println!(
        "bounced {:.2} s ({} frames) to {}; peak {:.3}",
        audio.frames() as f64 / rate.hz() as f64,
        audio.frames(),
        path.display(),
        audio.peak()
    );
    println!(
        "live playhead still at {:.1} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // The file reads back as the same 72_000 frames, 48_000 Hz, 2 channels.
    let (read, read_rate) = wav::read(&path)?;
    assert_eq!(read.frames(), 72_000);
    assert_eq!(read_rate.hz(), 48_000);
    assert_eq!(read.channels(), 2);
    println!(
        "read back: {} frames @ {} Hz, {} channels",
        read.frames(),
        read_rate.hz(),
        read.channels()
    );

    // 0.5 s .. 1.0 s is 24_000 frames, written as 32-bit float. The peak
    // is the same 0.321: that is where the whole bounce peaks.
    let overlap = export::bounce(
        &mut session,
        dir.join("export_overlap.wav"),
        Bounce {
            start: TimePosition::from_seconds(0.5, rate),
            length: Some(TimeDuration::from_seconds(0.5, rate)),
            format: WavFormat::Float32,
            ..Bounce::default()
        },
    )?;
    assert_eq!(overlap.frames(), 24_000);
    assert!((overlap.peak() - 0.321).abs() < 1e-3);
    println!("0.5 s .. 1.0 s as float: peak {:.3}", overlap.peak());

    // With `files`, the same 72_000 frames at 48 kHz become
    // `ceil(72_000 × 44_100 / 48_000)` = 66_150 frames at 44.1 kHz.
    #[cfg(feature = "files")]
    {
        let cd = dir.join("export_session_44k1.wav");
        let converted = export::bounce(
            &mut session,
            &cd,
            Bounce {
                sample_rate: Some(SampleRate::CD),
                ..Bounce::default()
            },
        )?;
        assert_eq!(converted.frames(), 66_150);
        println!(
            "resampled to 44.1 kHz: {} frames -> {}",
            converted.frames(),
            cd.display()
        );
    }
    Ok(())
}
