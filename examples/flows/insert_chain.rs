//! `insert` is the effects in a track's signal path, in order.
//!
//! ```text
//! insert::add / remove / move_to / param / bypass / reduction
//! ```
//!
//! ```text
//! let comp = insert::add(&mut session, vox, Effect::Compressor)?;
//! insert::param(&mut session, comp, EffectParam::Threshold, -24.0)?;
//! insert::bypass(&mut session, comp, true)?;
//! ```
//!
//! ```text
//! cargo run --example insert_chain
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::{Effect, EffectParam, TrackKind};
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::{clip, insert, session, track, transport};

/// The loudest sample once the effects have settled, played from the top.
fn level(session: &mut session::Session) -> Result<f32> {
    transport::rewind(session)?;
    transport::start(session)?;
    let audio = session::steps::render_blocks(session, 60)?;
    let settled = &audio.channel(0)[audio.frames() / 2..];
    Ok(settled.iter().fold(0.0f32, |m, s| m.max(s.abs())))
}

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let folder = std::env::temp_dir().join("insert_chain.superb");
    let _ = std::fs::remove_dir_all(&folder);
    let mut session = session::create(&folder, rate, 256)?;

    // One track playing a second of 220 Hz at 0.8.
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize);
    for ch in 0..2 {
        for (i, s) in tone.channel_mut(ch).iter_mut().enumerate() {
            *s = 0.8 * (core::f32::consts::TAU * 220.0 * i as f32 / rate.hz() as f32).sin();
        }
    }
    session.audio.insert("audio/tone.wav", tone);
    clip::add(&mut session, vox, "audio/tone.wav", TimePosition::ZERO)?;
    let dry = level(&mut session)?;
    println!("no inserts          peak {dry:.3}");

    // A compressor on the track. It starts at -18 dB and 4:1.
    let comp = insert::add(&mut session, vox, Effect::Compressor)?;
    let squeezed = level(&mut session)?;
    println!("compressor          peak {squeezed:.3}");
    assert!(squeezed < dry);

    // A parameter is heard at once and is one undo step per drag.
    insert::param(&mut session, comp, EffectParam::Threshold, -30.0)?;
    insert::param(&mut session, comp, EffectParam::Ratio, 10.0)?;
    let harder = level(&mut session)?;
    let off = insert::reduction(&session, comp).unwrap_or(0.0);
    println!("-30 dB, 10:1        peak {harder:.3}, taking off {off:.1} dB");
    assert!(harder < squeezed);

    // Bypass is state, like mute: the mixer is rebuilt without the insert.
    insert::bypass(&mut session, comp, true)?;
    let bypassed = level(&mut session)?;
    println!("bypassed            peak {bypassed:.3}");
    assert!((bypassed - dry).abs() < 1e-3);

    // A limiter on the master holds the whole mix under a ceiling (the
    // compressor is still bypassed).
    let master = session.memory.master().id();
    let limiter = insert::add(&mut session, master, Effect::Limiter)?;
    insert::param(&mut session, limiter, EffectParam::Ceiling, -12.0)?;
    let limited = level(&mut session)?;
    println!("limiter at -12 dBTP peak {limited:.3}");
    assert!(limited <= 0.252);

    // The chain is in the memory: undo takes the limiter's ceiling back,
    // then the limiter.
    session::undo(&mut session)?;
    session::undo(&mut session)?;
    assert_eq!(session.memory.master().inserts().len(), 0);
    insert::bypass(&mut session, comp, false)?;
    assert_eq!(session.memory.track(vox)?.inserts().len(), 1);

    session::save(&mut session)?;
    let again = session::open(&folder, 256)?;
    let kept = &again.memory.track(vox)?.inserts()[0];
    println!(
        "reopened: {:?}, threshold {:?} dB",
        kept.effect,
        kept.param(EffectParam::Threshold.id())
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
