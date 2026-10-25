//! Two seconds of 1 kHz at −12 dBFS: peak and true peak −12.00 dB, RMS
//! −15.04 dBFS, integrated −15.01 LUFS. Normalising to −16 LUFS applies
//! −0.99 dB. A copy delayed 20 ms lines up at 960 samples.
//!
//! ```text
//! cargo run --example measure_loudness
//! ```

use superb::primitives::analysis::{meter, offset, silence};
use superb::primitives::buffers::gain::{db_to_linear, linear_to_db};
use superb::primitives::buffers::PlanarBuffer;
use superb::primitives::{Result, SampleRate};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    let hz = rate.hz() as f32;
    assert_eq!(rate.seconds_to_samples(2.0), 96_000);

    // 96000 samples of 1 kHz at −12 dBFS. The first and last 480 samples
    // (10 ms) fade. A 1 kHz cycle is 48 samples, so sample 492 is a crest
    // past the fade and the tone reaches −12.00 dBFS.
    let mut tone: Vec<f32> = (0..2 * rate.hz() as usize)
        .map(|i| db_to_linear(-12.0) * (core::f32::consts::TAU * 1_000.0 * i as f32 / hz).sin())
        .collect();
    assert_eq!(tone.len(), 96_000);
    let n = tone.len();
    let fade = 480;
    assert!((rate.samples_to_seconds(fade as i64) - 0.01).abs() < 1e-9);
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        tone[i] *= g;
        tone[n - 1 - i] *= g;
    }
    let full = db_to_linear(-12.0);
    assert!((tone[492] - full).abs() < 1e-4);
    assert_eq!(format!("{:.2}", linear_to_db(full)), "-12.00");

    // The crest lands on a sample, so true peak equals the sample peak,
    // −12.00 dB. RMS of the faded tone is −15.04 dBFS.
    let peak_db = linear_to_db(meter::peak(&tone));
    let rms_db = linear_to_db(meter::rms(&tone));
    let true_peak_db = linear_to_db(meter::true_peak(&tone));
    assert_eq!(format!("{peak_db:.2}"), "-12.00");
    assert_eq!(format!("{rms_db:.2}"), "-15.04");
    assert_eq!(format!("{true_peak_db:.2}"), "-12.00");
    println!(
        "sample peak {:.2} dBFS, rms {:.2} dBFS, true peak {:.2} dBTP",
        peak_db, rms_db, true_peak_db
    );

    // Fed in blocks of 256. After 2.0 s, momentary (400 ms) is −15.08 LUFS.
    // Short-term needs 3 s, so it is absent. Integrated is −15.01 LUFS.
    let mut loudness = meter::Loudness::new(rate, 1)?;
    for block in tone.chunks(256) {
        loudness.push(&[block])?;
    }
    let momentary = loudness.momentary().unwrap_or(f32::NEG_INFINITY);
    let short_term = loudness.short_term().unwrap_or(f32::NEG_INFINITY);
    let integrated = loudness.integrated().unwrap_or(f32::NEG_INFINITY);
    assert_eq!(format!("{:.1}", loudness.seconds()), "2.0");
    assert!(loudness.short_term().is_none());
    assert_eq!(format!("{momentary:.2}"), "-15.08");
    assert_eq!(format!("{integrated:.2}"), "-15.01");
    println!(
        "after {:.1} s: momentary {:.2} LUFS, short-term {:.2} LUFS, integrated {:.2} LUFS",
        loudness.seconds(),
        momentary,
        short_term,
        integrated
    );

    // Target −16 LUFS under a −1 dBTP ceiling. The tone is −15.01 LUFS and
    // its true peak is −12 dBTP, so the gain is −0.99 dB and the ceiling
    // does not limit it. True peak lands at −12.99 dBTP.
    assert!((meter::PODCAST_LOUDNESS - -16.0).abs() < 1e-6);
    assert!((meter::TRUE_PEAK_CEILING - -1.0).abs() < 1e-6);
    let mut audio = PlanarBuffer::from_planes(&[&tone])?;
    let done = meter::normalize(
        &mut audio,
        rate,
        meter::PODCAST_LOUDNESS,
        meter::TRUE_PEAK_CEILING,
    )?
    .expect("the tone is long enough to normalise");
    assert_eq!(format!("{:+.2}", done.gain_db), "-0.99");
    assert_eq!(format!("{:.2}", done.loudness_before), "-15.01");
    assert_eq!(format!("{:.2}", done.loudness_after), "-16.00");
    assert_eq!(format!("{:.2}", done.true_peak_before), "-12.00");
    assert_eq!(format!("{:.2}", done.true_peak_after), "-12.99");
    println!(
        "normalised by {:+.2} dB: {:.2} -> {:.2} LUFS, true peak {:.2} -> {:.2} dBTP",
        done.gain_db,
        done.loudness_before,
        done.loudness_after,
        done.true_peak_before,
        done.true_peak_after
    );

    // The same tone arrives 20 ms later: 960 samples, inside a 100 ms
    // (4800-sample) search. The estimate returns that shift at confidence 0.96.
    let lag = (0.020 * hz) as usize;
    assert_eq!(lag, 960);
    let max_shift = rate.hz() as usize / 10;
    assert_eq!(max_shift, 4_800);
    assert!(lag < max_shift);
    let mut seed = 7u64;
    let mut noise = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    };
    let hiss: Vec<f32> = (0..n).map(|_| 0.2 * noise()).collect();
    let reference: Vec<f32> = tone.iter().zip(&hiss).map(|(t, h)| t + h).collect();
    let other: Vec<f32> = (0..n)
        .map(|i| {
            let late = if i >= lag { reference[i - lag] } else { 0.0 };
            0.5 * late + 0.05 * noise()
        })
        .collect();
    let found = offset::estimate(&reference, &other, max_shift)?;
    assert_eq!(found.shift, 960);
    assert_eq!(format!("{:.1}", found.shift as f32 / hz * 1_000.0), "20.0");
    assert_eq!(format!("{:.2}", found.confidence), "0.96");
    println!(
        "the second microphone lags by {} samples ({:.1} ms), confidence {:.2}",
        found.shift,
        found.shift as f32 / hz * 1_000.0,
        found.confidence
    );

    // The middle second, samples 24000..72000, is scaled by 0.0001. Sample
    // 72000 is a zero of the 1 kHz tone, so the stretch is 24000..72001,
    // which prints as 0.500 s to 1.500 s. The minimum length is 0.25 s.
    let mut with_gap = tone.clone();
    let gap = rate.hz() as usize / 2..rate.hz() as usize * 3 / 2;
    assert_eq!(gap, 24_000..72_000);
    for s in &mut with_gap[gap] {
        *s *= 0.000_1;
    }
    let min_len = rate.hz() as usize / 4;
    assert_eq!(min_len, 12_000);
    let ranges = silence::stretches(&with_gap, db_to_linear(-60.0), min_len);
    assert_eq!(ranges, vec![24_000..72_001]);
    for range in &ranges {
        assert_eq!(format!("{:.3}", range.start as f32 / hz), "0.500");
        assert_eq!(format!("{:.3}", range.end as f32 / hz), "1.500");
        println!(
            "silence from {:.3} s to {:.3} s",
            range.start as f32 / hz,
            range.end as f32 / hz
        );
    }
    // Trimming only cuts the ends. The fades cross −60 dBFS at sample 4
    // and sample 95995, so the span prints as 0.000 s .. 2.000 s.
    let kept = silence::trimmed(&with_gap, db_to_linear(-60.0));
    assert_eq!(kept.start, 4);
    assert_eq!(kept.end, 95_996);
    assert_eq!(format!("{:.3}", kept.start as f32 / hz), "0.000");
    assert_eq!(format!("{:.3}", kept.end as f32 / hz), "2.000");
    println!(
        "trimmed of silence at the ends: {:.3} s .. {:.3} s",
        kept.start as f32 / hz,
        kept.end as f32 / hz
    );
    Ok(())
}
