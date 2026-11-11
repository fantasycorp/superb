//! `transport` is play, stop, locate, loop.
//!
//! ```text
//! transport::start / stop / seek / rewind / looping / playhead / state
//! session::set_loop
//! ```
//!
//! ```text
//! transport::start(&mut session)?;
//! session::steps::render_blocks(&mut session, 47)?;
//! transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
//! session::set_loop(&mut session, Some(range))?;
//! transport::looping(&mut session, true)?;
//! transport::stop(&mut session)?;
//! transport::rewind(&mut session)?;
//! ```
//!
//! ```text
//! cargo run --example play_session
//! cargo run --example play_session --features devices   # also plays on speakers
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::transport::LoopRange;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("play_session.superb"), rate, 256)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;

    // 0.5 s of 330 Hz sine at 0.5, decaying e^(-6t).
    let frames = rate.hz() as usize / 2;
    let mut pluck = PlanarBuffer::new(2, frames);
    for ch in 0..2 {
        for (i, s) in pluck.channel_mut(ch).iter_mut().enumerate() {
            let t = i as f32 / rate.hz() as f32;
            *s = 0.5 * (-6.0 * t).exp() * (t * 330.0 * core::f32::consts::TAU).sin();
        }
    }
    session.audio.insert("audio/pluck.wav", pluck);
    // Three clips at 0, 0.5, 1.0 s.
    for n in 0..3 {
        clip::add(
            &mut session,
            vox,
            "audio/pluck.wav",
            TimePosition::from_seconds(0.5 * n as f64, rate),
        )?;
    }

    // start builds the mixer and rolls.
    transport::start(&mut session)?;
    let audio = session::steps::render_blocks(&mut session, 47)?;
    println!("state: {:?}", transport::state(&session));
    println!(
        "rendered {} frames, peak {:.3}, playhead now {:.3} s",
        audio.frames(),
        audio.peak(),
        transport::playhead(&session).to_seconds(rate)
    );

    // Seek to 1.0 s. Next block applies it.
    transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
    session::steps::render_blocks(&mut session, 1)?;
    println!(
        "after seek: {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // Loop 0.5..1.0 s.
    let range = LoopRange::new(
        TimePosition::from_seconds(0.5, rate),
        TimePosition::from_seconds(1.0, rate),
    )?;
    session::set_loop(&mut session, Some(range))?;
    transport::looping(&mut session, true)?;
    transport::seek(&mut session, range.start())?;
    session::steps::render_blocks(&mut session, 141)?;
    println!(
        "looping: {:.3} s (inside 0.5..1.0)",
        transport::playhead(&session).to_seconds(rate)
    );

    // stop applies on the next block. rewind, then one block, playhead is 0.
    transport::stop(&mut session)?;
    session::steps::render_blocks(&mut session, 1)?;
    println!("state: {:?}", transport::state(&session));
    transport::rewind(&mut session)?;
    session::steps::render_blocks(&mut session, 1)?;
    println!(
        "rewound to {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // Default output plays for 1.6 s.
    #[cfg(feature = "devices")]
    {
        use superb::devices;
        transport::looping(&mut session, false)?;
        devices::output(&mut session, None)?;
        transport::start(&mut session)?;
        std::thread::sleep(std::time::Duration::from_millis(1600));
        println!(
            "device playback: {:.3} s, load {:.1}%",
            transport::playhead(&session).to_seconds(rate),
            session.engine.load().load() * 100.0
        );
        transport::stop(&mut session)?;
    }
    Ok(())
}
