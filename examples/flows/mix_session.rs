//! `track` and `send`: two tones, a fader, pan, mute, solo, then a bus.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example mix_session
//! ```

use superb::primitives::buffers::gain::db_to_linear;
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::mixer::{PanLaw, SendTap};
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::session::Session;
use superb::{clip, send, session, track, transport};

/// Four blocks of 128. The peak of channel 0 on the last block: a move has
/// had three blocks to finish its ramp.
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

    // Four seconds of DC at 0.25 on both channels, one clip on each track
    // from sample 0. Strips start on the constant-power law; Balance makes
    // centre unity, so each track contributes 0.25 and the master sums to
    // 0.500. −6.0206 dB is half in `f32`.
    let mut tone = PlanarBuffer::new(2, rate.hz() as usize * 4);
    tone.fill(0.25);
    session.audio.insert("tone.wav", tone);
    for t in [vox, gtr] {
        clip::add(&mut session, t, "tone.wav", TimePosition::ZERO)?;
    }
    transport::start(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    let half = db_to_linear(-6.0206);
    assert!((half - 0.5).abs() < 1e-6);
    let both = level(&mut session)?;
    assert!((both - 0.5).abs() < 1e-4);
    println!("both tracks at 0 dB:     {:.3}  (0.25 + 0.25)", both);

    // Three fader moves, −2 then −4 then −6.0206 dB, inside one undo step.
    // The stack already held the two tracks and the two clips, so its
    // length is 5. gtr is half of 0.25. The master is 0.25 + 0.125 = 0.375.
    for db in [-2.0, -4.0, -6.0206] {
        track::volume(&mut session, gtr, db)?;
    }
    session.history.seal();
    assert_eq!(session.history.undo_len(), 5);
    assert!((session.memory.track(gtr)?.state.volume_db + 6.0206).abs() < 1e-6);
    let faded = level(&mut session)?;
    assert!((faded - (0.25 + 0.25 * half)).abs() < 1e-4);
    println!("gtr at -6 dB:            {:.3}  (0.25 + 0.125)", faded);
    println!("undo steps so far: {}", session.history.undo_len());

    // vox hard left: Balance gives it (1, 0), so the last sample is
    // L 0.375 (0.25 + 0.125) and R 0.125 (gtr alone).
    track::pan(&mut session, vox, -1.0)?; // hard left
    let audio = session::steps::render_blocks(&mut session, 4)?;
    let n = audio.frames() - 1;
    assert!((audio.channel(0)[n] - (0.25 + 0.25 * half)).abs() < 1e-4);
    assert!((audio.channel(1)[n] - 0.25 * half).abs() < 1e-4);
    println!(
        "vox hard left:           L {:.3}  R {:.3}",
        audio.channel(0)[n],
        audio.channel(1)[n]
    );
    // Pan back to centre. Mute leaves vox: 0.250. Solo leaves gtr: 0.125.
    track::pan(&mut session, vox, 0.0)?;
    track::mute(&mut session, gtr, true)?;
    let muted = level(&mut session)?;
    assert!((muted - 0.25).abs() < 1e-4);
    println!("gtr muted:               {:.3}", muted);
    track::mute(&mut session, gtr, false)?;
    track::solo(&mut session, gtr, true)?;
    let soloed = level(&mut session)?;
    assert!((soloed - 0.25 * half).abs() < 1e-4);
    println!("gtr soloed:              {:.3}  (vox silenced)", soloed);
    track::solo(&mut session, gtr, false)?;

    // A bus, "room", and a post-fader send from each track at −6.0206 dB.
    // Each send adds half of that track on top of the direct path:
    // 0.25 × (1 + ½)² = 0.5625, which prints as 0.562.
    let room = track::add(&mut session, "room", TrackKind::Bus)?;
    let vox_send = send::add(&mut session, vox, room, -6.0206, SendTap::PostFader)?;
    let _gtr_send = send::add(&mut session, gtr, room, -6.0206, SendTap::PostFader)?;
    session::steps::rebuild(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    let with_sends = level(&mut session)?;
    assert!((with_sends - 0.25 * (1.0 + half) * (1.0 + half)).abs() < 1e-4);
    println!(
        "plus bus with two sends: {:.3}  (tracks + half of each)",
        with_sends
    );
    // vox's send goes to −12.04 dB (a quarter, 0.25003). The master is
    // 0.25 + 0.125 + 0.25×0.25003 + 0.0625 = 0.500.
    send::level(&mut session, vox_send, -12.04)?;
    let quarter = db_to_linear(-12.04);
    let at_12 = level(&mut session)?;
    let at_12_expected = 0.25 + 0.25 * half + 0.25 * quarter + (0.25 * half) * half;
    assert!((at_12 - at_12_expected).abs() < 1e-4);
    println!("vox send at -12 dB:      {:.3}", at_12);
    // vox's send off. Left: vox 0.25, gtr 0.125, gtr's send 0.0625.
    // 0.4375 prints as 0.438.
    send::enable(&mut session, vox_send, false)?;
    let send_off = level(&mut session)?;
    assert!((send_off - (0.25 + 0.25 * half + (0.25 * half) * half)).abs() < 1e-4);
    println!("vox send off:            {:.3}", send_off);

    // gtr's output now feeds the bus, and its post-fader send still does
    // too. The bus hears 0.125 + 0.0625, then its fader halves that to
    // 0.09375. The master is vox's 0.25 plus that: 0.34375, printed 0.344.
    // The room meter reads the bus, 0.094 on each side.
    track::route(&mut session, gtr, Some(room))?;
    session::steps::rebuild(&mut session)?;
    for (_, strip) in &session.mixer.strips {
        strip.set_pan_law(PanLaw::Balance);
    }
    track::volume(&mut session, room, -6.0206)?;
    let gtr_level = 0.25 * half;
    let bus = (gtr_level + gtr_level * half) * half;
    let via_bus = level(&mut session)?;
    assert!((via_bus - (0.25 + bus)).abs() < 1e-4);
    println!("gtr via bus at -6 dB:    {:.3}", via_bus);
    let [l, r] = track::peak(&session, room).expect("room meter");
    assert!((l - bus).abs() < 1e-4);
    assert!((r - bus).abs() < 1e-4);
    println!("room bus meter:          L {l:.3}  R {r:.3}");

    // Undo back to the two tracks and the two clips. gtr is 0 dB again,
    // `track_count` is 2, and gtr has no sends.
    while session.history.undo_len() > 4 {
        // 4 = the two tracks and two clips we started with
        session::undo(&mut session)?;
    }
    assert_eq!(session.history.undo_len(), 4);
    assert_eq!(session.memory.track(gtr)?.state.volume_db, 0.0);
    assert_eq!(session.memory.track_count(), 2);
    assert_eq!(session.memory.track(gtr)?.sends().len(), 0);
    println!(
        "after undo: gtr {} dB, {} tracks, gtr sends {}",
        session.memory.track(gtr)?.state.volume_db,
        session.memory.track_count(),
        session.memory.track(gtr)?.sends().len()
    );
    Ok(())
}
