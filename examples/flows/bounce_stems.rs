//! `session::Root` is where sessions live. `session::Layout` is the files
//! inside one folder. `export::stems` writes every strip to its own file.
//!
//! ```text
//! Root::default / Root::new / Root::folder / Root::sessions
//! session::layout -> Layout
//! export::bounce / export::stems
//! ```
//!
//! ```text
//! let folder = Root::new(path).folder("Bounce Stems");
//! let mut session = session::create(&folder, rate, 256)?;
//! let layout = session::layout(&session);
//! export::bounce(&mut session, layout.bounce("rough mix"), Bounce::default())?;
//! export::stems(&mut session, "final", Stems::default())?;
//! ```
//!
//! ```text
//! cargo run --example bounce_stems
//! ```

use superb::export::{self, Bounce, StemSource, Stems};
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::session::TrackKind;
use superb::primitives::{Result, SampleRate, TimePosition};
use superb::session::{Layout, Root};
use superb::{clip, session, track};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;

    // Default root is ~/superb. This run uses a temp root and folder "Bounce Stems".
    let home = Root::default();
    println!("sessions live in {} by default", home.path().display());
    let root = Root::new(std::env::temp_dir().join("superb-example"));
    let folder = root.folder("Bounce Stems");
    let _ = std::fs::remove_dir_all(&folder); // a fresh run every time
    let mut session = session::create(&folder, rate, 256)?;
    println!(
        "sessions under {}: {:?}",
        root.path().display(),
        root.sessions()?
    );

    // Layout names memory.json, state.json, audio/, bounces/, stems/.
    let layout: Layout = session::layout(&session);
    println!("memory  {}", layout.memory().display());
    println!(
        "state   {} (written once session::state_file names it)",
        layout.state().display()
    );
    println!("audio   {}", layout.audio().display());
    println!("bounces {}", layout.bounces().display());
    println!("stems   {}", layout.stems().display());

    // host, guest, music: 1 s tones. guest starts at 0.5 s. music routes to beds.
    let host = track::add(&mut session, "host", TrackKind::Audio)?;
    let guest = track::add(&mut session, "guest", TrackKind::Audio)?;
    let music = track::add(&mut session, "music", TrackKind::Audio)?;
    let beds = track::add(&mut session, "beds", TrackKind::Bus)?;
    track::route(&mut session, music, Some(beds))?;
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
        session.audio.insert(format!("audio/{name}.wav"), tone);
    }
    clip::add(&mut session, host, "audio/host.wav", TimePosition::ZERO)?;
    clip::add(
        &mut session,
        guest,
        "audio/guest.wav",
        TimePosition::from_seconds(0.5, rate),
    )?;
    clip::add(&mut session, music, "audio/music.wav", TimePosition::ZERO)?;

    // Mix to bounces/rough mix.wav. Stem set "final".
    let rough = layout.bounce("rough mix");
    export::bounce(&mut session, &rough, Bounce::default())?;
    let set = export::stems(&mut session, "final", Stems::default())?;
    println!(
        "\nstem set {:?}: {} Hz, {}-bit, {} frames from sample {}",
        set.name, set.sample_rate, set.bits, set.frames, set.start
    );
    for stem in set.stems.iter().chain(std::iter::once(&set.mix)) {
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
    println!(
        "manifest {}",
        layout.stem_set("final").join("manifest.json").display()
    );
    Ok(())
}
