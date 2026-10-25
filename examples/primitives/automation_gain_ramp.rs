//! Read mode fills a gain that runs from 0 at sample 0 to 1 at sample 200,
//! then holds. Each 32-sample block stores its first sample in the atomic.
//!
//! ```text
//! cargo run --example automation_gain_ramp
//! ```

use std::sync::Arc;

use superb::primitives::automation::{
    advance_block, AutomationLane, AutomationMode, EnvelopePoint,
};
use superb::primitives::realtime::AtomicF32;
use superb::primitives::{IdGenerator, TimePosition};

fn main() {
    let ids = IdGenerator::new();
    let gain = Arc::new(AtomicF32::new(0.0));
    let mut lane = AutomationLane::new(ids.next(), Arc::clone(&gain));

    // Linear from sample 0 to sample 200, then the last point holds.
    lane.envelope_mut()
        .upsert(EnvelopePoint::linear(TimePosition::from_samples(0), 0.0));
    lane.envelope_mut()
        .upsert(EnvelopePoint::linear(TimePosition::from_samples(200), 1.0));
    lane.set_mode(AutomationMode::Read);
    assert!(lane.evaluate(TimePosition::from_samples(0)).abs() < 1e-6);
    assert!((lane.evaluate(TimePosition::from_samples(200)) - 1.0).abs() < 1e-6);
    assert!((lane.evaluate(TimePosition::from_samples(256)) - 1.0).abs() < 1e-6);

    let block = 32usize;
    let mut pos = TimePosition::ZERO;
    let mut scratch = vec![0.0f32; block];
    // Eight blocks. The atomic matches the first sample. Printed rows:
    // t=0 0.000..0.155, 32 0.160..0.315, 64 0.320..0.475, 96 0.480..0.635,
    // 128 0.640..0.795, 160 0.800..0.955, 192 0.960..1.000, 224 1.000..1.000.
    let rows = [
        (0, "0.000", "0.155"),
        (32, "0.160", "0.315"),
        (64, "0.320", "0.475"),
        (96, "0.480", "0.635"),
        (128, "0.640", "0.795"),
        (160, "0.800", "0.955"),
        (192, "0.960", "1.000"),
        (224, "1.000", "1.000"),
    ];

    println!("sample-accurate gain ramp (Read mode), block={block}");
    for (i, _) in rows.iter().enumerate() {
        lane.apply_block(pos, block, true, Some(&mut scratch));
        let first = scratch[0];
        let last = scratch[block - 1];
        let sample = pos.samples();
        let ramp = |s: i64| -> f32 {
            if s >= 200 {
                1.0
            } else {
                s as f32 / 200.0
            }
        };
        assert_eq!(sample, rows[i].0);
        assert!((first - ramp(sample)).abs() < 1e-6);
        assert!((last - ramp(sample + block as i64 - 1)).abs() < 1e-6);
        assert!((gain.get() - first).abs() < 1e-6);
        assert_eq!(format!("{first:.3}"), rows[i].1);
        assert_eq!(format!("{last:.3}"), rows[i].2);
        assert_eq!(format!("{:.3}", gain.get()), rows[i].1);

        // Twenty cells, one hash per 0.05 of the block's last sample.
        let bar = |v: f32| {
            let n = (v * 20.0).round().clamp(0.0, 20.0) as usize;
            format!("{}{}", "#".repeat(n), ".".repeat(20 - n))
        };
        let drawn = bar(last);
        assert_eq!(drawn.len(), 20);
        assert_eq!(
            drawn.matches('#').count(),
            (last * 20.0).round().clamp(0.0, 20.0) as usize
        );
        println!(
            "t={:>4}  atomic={:.3}  block=[{:.3} .. {:.3}]  {}",
            pos.samples(),
            gain.get(),
            first,
            last,
            drawn
        );
        pos = advance_block(pos, block);
    }
    // Eight blocks of 32 end at sample 256, on the hold.
    assert_eq!(pos.samples(), 256);
    assert!((gain.get() - 1.0).abs() < 1e-6);
}
