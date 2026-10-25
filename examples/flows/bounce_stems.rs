//! `session::Layout`, `session::Root` and `export::stems`: one folder shape,
//! then a stem set of four strips and the mix.
//!
//! Each comment is the block under it.
//!
//! ```text
//! cargo run --example bounce_stems
//! ```

use superb::export::{self, Bounce, StemSource, Stems};
use superb::primitives::buffers::gain::linear_to_db;
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::mixer::PanLaw;
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::session::{Layout, Root};
use superb::{clip, session, track};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;

    // The default root is `superb` in the home directory. This run uses a
    // root in the temp directory and a session folder named "Bounce Stems".
    let home = Root::default();
    assert!(home.path().ends_with("superb"));
    println!("sessions live in {} by default", home.path().display());
    let root = Root::new(std::env::temp_dir().join("superb-example"));
    let folder = root.folder("Bounce Stems");
    let _ = std::fs::remove_dir_all(&folder); // a fresh run every time
    let mut session = session::create(&folder, rate, 256)?;
    let found = root.sessions()?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0], folder);
    println!("sessions under {}: {:?}", root.path().display(), found);

    // `memory.json`, `state.json`, `audio/`, `bounces/`, `stems/`.
    let layout: Layout = session::layout(&session);
    assert_eq!(layout.memory(), folder.join("memory.json"));
    assert_eq!(layout.state(), folder.join("state.json"));
    assert_eq!(layout.audio(), folder.join("audio"));
    assert_eq!(layout.bounces(), folder.join("bounces"));
    assert_eq!(layout.stems(), folder.join("stems"));
    println!("memory  {}", layout.memory().display());
    println!(
        "state   {} (written once session::state_file names it)",
        layout.state().display()
    );
    println!("audio   {}", layout.audio().display());
    println!("bounces {}", layout.bounces().display());
    println!("stems   {}", layout.stems().display());

    // host, guest and music are audio tracks; beds is a bus and music
    // feeds it. Each tone is one second, one channel: host 180 Hz at 0.4,
    // guest 240 Hz at 0.3, music 440 Hz at 0.2. host and music start at
    // sample 0, guest at sample 24_000, so the session ends at sample
    // 72_000 (1.5 s).
    let host = track::add(&mut session, "host", TrackKind::Audio)?;
    let guest = track::add(&mut session, "guest", TrackKind::Audio)?;
    let music = track::add(&mut session, "music", TrackKind::Audio)?;
    let beds = track::add(&mut session, "beds", TrackKind::Bus)?;
    track::route(&mut session, music, Some(beds))?;
    assert_eq!(session.memory.track(music)?.output(), Some(beds));
    for (name, hz, level) in [
        ("host", 180.0, 0.4),
        ("guest", 240.0, 0.3),
        ("music", 440.0, 0.2),
    ] {
        let frames = rate.hz() as usize; // a second each
        let mut tone = PlanarBuffer::new(1, frames);
        for (i, s) in tone.channel_mut(0).iter_mut().enumerate() {
            *s = level * (core::f32::consts::TAU * hz * i as f32 / rate.hz() as f32).sin();
        }
        session.audio.insert(format!("{name}.wav"), tone);
    }
    for (name, hz, level) in [
        ("host", 180.0f32, 0.4f32),
        ("guest", 240.0, 0.3),
        ("music", 440.0, 0.2),
    ] {
        let key = format!("{name}.wav");
        let tone = session.audio.get(std::path::Path::new(&key)).expect("tone");
        assert_eq!(tone.frames(), 48_000);
        assert_eq!(tone.channels(), 1);
        let i = 200usize;
        let expected = level * (core::f32::consts::TAU * hz * i as f32 / rate.hz() as f32).sin();
        assert!((tone.channel(0)[i] - expected).abs() < 1e-5);
    }
    clip::add(&mut session, host, "host.wav", TimePosition::ZERO)?;
    clip::add(
        &mut session,
        guest,
        "guest.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;
    clip::add(&mut session, music, "music.wav", TimePosition::ZERO)?;
    assert_eq!(session.memory.track(host)?.clips()[0].start().samples(), 0);
    assert_eq!(
        session.memory.track(guest)?.clips()[0].start().samples(),
        24_000
    );
    assert_eq!(session.memory.track(music)?.clips()[0].start().samples(), 0);
    for id in [host, guest, music] {
        assert_eq!(
            session.memory.track(id)?.clips()[0].length().samples(),
            48_000
        );
    }

    // `bounces/rough mix.wav` is the mix. The stem set "final" is 24-bit,
    // 48 kHz, 72_000 frames from sample 0: host, guest and music after the
    // centre constant-power pan (−11.0, −13.5 and −17.0 dBFS), beds at the
    // same level as music, and the mix at −4.2 dBFS. Loudness is −12.7,
    // −15.0, −18.4, −18.4 and −10.8 LUFS.
    let rough = layout.bounce("rough mix");
    assert_eq!(rough, folder.join("bounces").join("rough mix.wav"));
    export::bounce(&mut session, &rough, Bounce::default())?;
    assert!(rough.is_file());
    let set = export::stems(&mut session, "final", Stems::default())?;
    assert_eq!(set.name, "final");
    assert_eq!(set.sample_rate, 48_000);
    assert_eq!(set.bits, 24);
    assert_eq!(set.frames, 72_000);
    assert_eq!(set.start, 0);
    assert_eq!(set.stems.len(), 4);
    println!(
        "\nstem set {:?}: {} Hz, {}-bit, {} frames from sample {}",
        set.name, set.sample_rate, set.bits, set.frames, set.start
    );
    let centre = PanLaw::ConstantPower.gains(0.0).0;
    let track_db = |amp: f32| linear_to_db(amp * centre);
    for stem in set.stems.iter().chain(std::iter::once(&set.mix)) {
        assert_eq!(stem.channels, 2);
        let (source, output, peak, lufs) = match stem.file.as_str() {
            "host.wav" => (StemSource::Audio, None, track_db(0.4), -12.7),
            "guest.wav" => (StemSource::Audio, None, track_db(0.3), -15.0),
            "music.wav" => (StemSource::Audio, Some("beds"), track_db(0.2), -18.4),
            "beds.wav" => (StemSource::Bus, None, track_db(0.2), -18.4),
            "mix.wav" => (StemSource::Mix, None, -4.2, -10.8),
            other => panic!("unexpected stem {other}"),
        };
        assert_eq!(stem.source, source);
        assert_eq!(stem.output.as_deref(), output);
        let got = stem.peak_dbfs.expect("peak");
        let tolerance = if stem.file == "mix.wav" { 0.05 } else { 0.02 };
        assert!(
            (got - peak).abs() < tolerance,
            "{} peak {got} vs {peak}",
            stem.file
        );
        let true_peak = stem.true_peak_dbtp.expect("true peak");
        assert!((true_peak - got).abs() < 0.05);
        let loud = stem.loudness_lufs.expect("loudness");
        assert!(
            (loud - lufs).abs() < 0.05,
            "{} loudness {loud} vs {lufs}",
            stem.file
        );
        let to = match (stem.source, stem.output.as_deref()) {
            (StemSource::Mix, _) => "out",
            (_, Some(bus)) => bus,
            (_, None) => "master",
        };
        println!(
            "  {:<10} {:<5} to {:<7} peak {:>6.1} dBFS, true peak {:>6.1} dBTP, {}",
            stem.file,
            format!("{:?}", stem.source),
            to,
            stem.peak_dbfs.unwrap_or(f32::NEG_INFINITY),
            stem.true_peak_dbtp.unwrap_or(f32::NEG_INFINITY),
            stem.loudness_lufs
                .map_or("too short for LUFS".to_string(), |l| format!("{l:.1} LUFS"))
        );
    }
    let manifest = layout.stem_set("final").join("manifest.json");
    assert!(manifest.is_file());
    println!("manifest {}", manifest.display());
    Ok(())
}
