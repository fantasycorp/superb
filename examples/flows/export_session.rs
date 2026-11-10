//! `export::bounce` writes the session, or a range of it, to a WAV file.
//!
//! ```text
//! export::bounce(session, path, Bounce) -> PlanarBuffer
//! ```
//!
//! ```text
//! export::bounce(&mut session, &path, Bounce::default())?;
//! export::bounce(&mut session, &path, Bounce {
//!     start: TimePosition::from_seconds(0.5, rate),
//!     length: Some(TimeDuration::from_seconds(0.5, rate)),
//!     format: WavFormat::Float32,
//!     ..Bounce::default()
//! })?;
//! ```
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

    // lead: 440 Hz at 0.3 from 0. pad: 110 Hz at 0.2 from 0.5 s, fader −6 dB.
    // lead pan 0.3.
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
    session.audio.insert("audio/a.wav", a);
    session.audio.insert("audio/b.wav", b);
    clip::add(&mut session, lead, "audio/a.wav", TimePosition::ZERO)?;
    clip::add(
        &mut session,
        pad,
        "audio/b.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;
    track::volume(&mut session, pad, -6.0)?;
    track::pan(&mut session, lead, 0.3)?;

    // Default bounce: 16-bit PCM, 0 to last clip. Playhead stays at 0.
    let dir = std::env::temp_dir();
    let path = dir.join("export_session.wav");
    let audio = export::bounce(&mut session, &path, Bounce::default())?;
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

    let (read, read_rate) = wav::read(&path)?;
    println!(
        "read back: {} frames @ {} Hz, {} channels",
        read.frames(),
        read_rate.hz(),
        read.channels()
    );

    // 0.5..1.0 s as float.
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
    println!("0.5 s .. 1.0 s as float: peak {:.3}", overlap.peak());

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
        println!(
            "resampled to 44.1 kHz: {} frames -> {}",
            converted.frames(),
            cd.display()
        );
    }
    Ok(())
}
