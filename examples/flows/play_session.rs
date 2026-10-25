//! `transport`: three plucks, a seek, a loop, a stop, a rewind.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example play_session
//! cargo run --example play_session --features devices   # also plays on speakers
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::transport::{LoopRange, TransportState};
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, session, track, transport};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("play_session.superb"), rate, 256)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;

    // Half a second (24_000 frames) of a 330 Hz sine, amplitude 0.5, with
    // decay `e^(-6t)`. Both channels hold that. The clip is named "pluck".
    let frames = rate.hz() as usize / 2;
    let mut pluck = PlanarBuffer::new(2, frames);
    for ch in 0..2 {
        for (i, s) in pluck.channel_mut(ch).iter_mut().enumerate() {
            let t = i as f32 / rate.hz() as f32;
            *s = 0.5 * (-6.0 * t).exp() * (t * 330.0 * core::f32::consts::TAU).sin();
        }
    }
    session.audio.insert("pluck.wav", pluck);
    let stored = session
        .audio
        .get(std::path::Path::new("pluck.wav"))
        .expect("pluck");
    assert_eq!(stored.frames(), 24_000);
    let i = 36usize;
    let t = i as f32 / rate.hz() as f32;
    let sample = 0.5 * (-6.0 * t).exp() * (t * 330.0 * core::f32::consts::TAU).sin();
    assert!((stored.channel(0)[i] - sample).abs() < 1e-6);
    assert!((stored.channel(1)[i] - sample).abs() < 1e-6);
    // Three clips, at 0, 0.5 and 1.0 s. Each is 24_000 samples, so they
    // meet at 0.5 s and 1.0 s. The session runs to 1.5 s.
    for n in 0..3 {
        clip::add(
            &mut session,
            vox,
            "pluck.wav",
            TimePosition::from_seconds(0.5 * n as f64, rate),
        )?;
    }
    let placed = session.memory.track(vox)?.clips();
    assert_eq!(placed.len(), 3);
    for (n, c) in placed.iter().enumerate() {
        assert_eq!(c.name, "pluck");
        assert_eq!(c.start().samples(), 24_000 * n as i64);
        assert_eq!(c.length().samples(), 24_000);
    }

    // `start` builds the mixer and rolls. 47 blocks of 256 are 12_032
    // frames, 0.250667 s, which prints as 0.251 s. Only the first pluck
    // is in that window. Its peak through the centre constant-power pan
    // is 0.352. The transport is Playing.
    transport::start(&mut session)?;
    let audio = session::steps::render_blocks(&mut session, 47)?;
    assert_eq!(audio.frames(), 12_032);
    assert!((audio.peak() - 0.352).abs() < 1e-3);
    assert_eq!(transport::playhead(&session).samples(), 12_032);
    assert_eq!(transport::state(&session), TransportState::Playing);
    println!("state: {:?}", transport::state(&session));
    println!(
        "rendered {} frames, peak {:.3}, playhead now {:.3} s",
        audio.frames(),
        audio.peak(),
        transport::playhead(&session).to_seconds(rate)
    );

    // Seek to 1.0 s (sample 48_000). The next block applies it and then
    // plays 256 samples, so the playhead is sample 48_256 (1.005 s).
    transport::seek(&mut session, TimePosition::from_seconds(1.0, rate))?;
    session::steps::render_blocks(&mut session, 1)?;
    assert_eq!(transport::playhead(&session).samples(), 48_256);
    println!(
        "after seek: {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // Loop 0.5 s..1.0 s (samples 24_000..48_000), 24_000 samples long.
    // Seek back to the start of it and render 141 blocks: 36_096 samples,
    // 0.752 s. That is one trip around the loop plus 12_096 samples, so
    // the playhead finishes at sample 36_096 (0.752 s).
    let range = LoopRange::new(
        TimePosition::from_seconds(0.5, rate),
        TimePosition::from_seconds(1.0, rate),
    )?;
    session::set_loop(&mut session, Some(range))?;
    transport::looping(&mut session, true)?;
    transport::seek(&mut session, range.start())?;
    session::steps::render_blocks(&mut session, 141)?;
    assert_eq!(transport::playhead(&session).samples(), 36_096);
    println!(
        "looping: {:.3} s (inside 0.5..1.0)",
        transport::playhead(&session).to_seconds(rate)
    );

    // Stop takes effect on the next block and leaves the playhead at
    // sample 36_096. Rewind, then one block, and the playhead is 0.
    transport::stop(&mut session)?;
    session::steps::render_blocks(&mut session, 1)?; // the Stop command is applied at the next block
    assert_eq!(transport::state(&session), TransportState::Stopped);
    assert_eq!(transport::playhead(&session).samples(), 36_096);
    println!("state: {:?}", transport::state(&session));
    transport::rewind(&mut session)?;
    session::steps::render_blocks(&mut session, 1)?;
    assert_eq!(transport::playhead(&session).samples(), 0);
    println!(
        "rewound to {:.3} s",
        transport::playhead(&session).to_seconds(rate)
    );

    // With `devices`, looping goes off, the default output takes the
    // engine, and `start` plays for 1_600 ms. The device pulls the blocks;
    // the print is the playhead and the engine load after that sleep.
    #[cfg(feature = "devices")]
    {
        use superb::devices;
        transport::looping(&mut session, false)?;
        devices::output(&mut session, None)?; // the default output, matched to the session
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
