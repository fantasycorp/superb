//! `track` is add, volume, pan, mute, solo, route. `send` is a copy of a
//! track into a bus.
//!
//! ```text
//! track::add / volume / pan / mute / solo / route / peak
//! send::add / level / enable
//! ```
//!
//! ```text
//! track::volume(&mut session, gtr, -6.0)?;
//! track::pan(&mut session, vox, -1.0)?;
//! track::mute(&mut session, gtr, true)?;
//! send::add(&mut session, vox, room, -6.0, SendTap::PostFader)?;
//! track::route(&mut session, gtr, Some(room))?;
//! ```
//!
//! ```text
//! cargo run --example mix_session
//! ```

use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::mixer::{PanLaw, SendTap};
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::session::Session;
use superb::{clip, send, session, track, transport};

/// Peak of channel 0 on the last of 4 blocks (a fader ramp has finished).
fn level(session: &mut Session) -> Result<f32> {
    let audio = session::steps::render_blocks(session, 4)?;
    let last = audio.frames() - session.engine.max_frames();
    Ok(audio.channel(0)[last..]
        .iter()
        .fold(0.0f32, |m, s| m.max(s.abs())))
}

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let mut session = session::create(std::env::temp_dir().join("mix_session.superb"), rate, 128)?;
    let vox = track::add(&mut session, "vox", TrackKind::Audio)?;
    let gtr = track::add(&mut session, "gtr", TrackKind::Audio)?;

    // 4 s of DC 0.25 on both tracks. Balance pan so centre is unity.
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize * 4);
    tone.fill(0.25);
    session.audio.insert("audio/tone.wav", tone);
    for t in [vox, gtr] {
        clip::add(&mut session, t, "audio/tone.wav", TimePosition::ZERO)?;
    }
    transport::start(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    println!("both tracks at 0 dB:     {:.3}", level(&mut session)?);

    // gtr fader to −6 dB. Moves in one undo step.
    for db in [-2.0, -4.0, -6.0] {
        track::volume(&mut session, gtr, db)?;
    }
    session.history.seal();
    println!("gtr at -6 dB:            {:.3}", level(&mut session)?);
    println!("undo steps so far: {}", session.history.undo_len());

    // vox hard left.
    track::pan(&mut session, vox, -1.0)?;
    let audio = session::steps::render_blocks(&mut session, 4)?;
    let n = audio.frames() - 1;
    println!(
        "vox hard left:           L {:.3}  R {:.3}",
        audio.channel(0)[n],
        audio.channel(1)[n]
    );
    track::pan(&mut session, vox, 0.0)?;
    track::mute(&mut session, gtr, true)?;
    println!("gtr muted:               {:.3}", level(&mut session)?);
    track::mute(&mut session, gtr, false)?;
    track::solo(&mut session, gtr, true)?;
    println!("gtr soloed:              {:.3}", level(&mut session)?);
    track::solo(&mut session, gtr, false)?;

    // Bus "room". Post-fader send from each at −6 dB.
    let room = track::add(&mut session, "room", TrackKind::Bus)?;
    let vox_send = send::add(&mut session, vox, room, -6.0, SendTap::PostFader)?;
    let _gtr_send = send::add(&mut session, gtr, room, -6.0, SendTap::PostFader)?;
    session::steps::rebuild(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    println!("plus bus with two sends: {:.3}", level(&mut session)?);
    send::level(&mut session, vox_send, -12.0)?;
    println!("vox send at -12 dB:      {:.3}", level(&mut session)?);
    send::enable(&mut session, vox_send, false)?;
    println!("vox send off:            {:.3}", level(&mut session)?);

    // gtr through the bus; bus fader −6 dB.
    track::route(&mut session, gtr, Some(room))?;
    session::steps::rebuild(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    track::volume(&mut session, room, -6.0)?;
    println!("gtr via bus at -6 dB:    {:.3}", level(&mut session)?);
    let [l, r] = track::peak(&session, room).expect("room meter");
    println!("room bus meter:          L {l:.3}  R {r:.3}");

    // Undo back to two tracks and two clips.
    while session.history.undo_len() > 4 {
        session::undo(&mut session)?;
    }
    println!(
        "after undo: gtr {} dB, {} tracks, gtr sends {}",
        session.memory.track(gtr)?.state.volume_db,
        session.memory.track_count(),
        session.memory.track(gtr)?.sends().len()
    );
    Ok(())
}
