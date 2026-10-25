//! 120 bpm in 4/4, 90 bpm from beat 8, 3/4 from bar 4. A 256-frame block
//! that starts 100 frames before the loop end splits into 100 frames and
//! 156 frames wrapped back to beat 4.
//!
//! ```text
//! cargo run --example transport_tempo_map
//! ```

use superb::primitives::transport::{LoopRange, TempoMap, Transport};
use superb::primitives::{BeatPosition, Result, SampleRate, TimePosition, TimeSignature};

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    assert_eq!(rate.seconds_to_samples(1.0), 48_000);
    // 120 bpm until beat 8, then 90. 4/4 until bar 4 (beat 12), then 3/4.
    let mut map = TempoMap::new(rate, 120.0);
    map.add_tempo(BeatPosition::new(8.0), 90.0)?;
    map.add_meter(4, TimeSignature::new(3, 4)?)?;

    // Beat 7.5 is bar 2, beat 4, tick 480. Beat 12 is the first 3/4 bar.
    // Beat 15 is the next bar. 4 beats at 90 bpm from second 4.000 is 6.667 s.
    println!("beat   bar.beat.tick   seconds   samples   bpm");
    let rows = [
        (0.0, "1.1.000", "0.000", 0, "120"),
        (4.0, "2.1.000", "2.000", 96_000, "120"),
        (7.5, "2.4.480", "3.750", 180_000, "120"),
        (8.0, "3.1.000", "4.000", 192_000, "90"),
        (12.0, "4.1.000", "6.667", 320_000, "90"),
        (15.0, "5.1.000", "8.667", 416_000, "90"),
    ];
    for (beat, bbt, seconds, samples, bpm) in rows {
        let b = BeatPosition::new(beat);
        assert_eq!(map.beat_to_bbt(b).to_string(), bbt);
        assert_eq!(format!("{:.3}", map.beat_to_seconds(b)), seconds);
        assert_eq!(map.beat_to_samples(b).samples(), samples);
        assert_eq!(format!("{:.0}", map.tempo_at(b)), bpm);
        println!(
            "{:>4}   {:<14}  {:>7.3}   {:>7}   {:>3}",
            beat,
            map.beat_to_bbt(b).to_string(),
            map.beat_to_seconds(b),
            map.beat_to_samples(b).samples(),
            map.tempo_at(b)
        );
    }
    // Two seconds is still inside the 120 bpm region: beat 4.000.
    let two_seconds = TimePosition::from_seconds(2.0, rate);
    let beat = map.samples_to_beat(two_seconds).beats();
    assert_eq!(format!("{beat:.3}"), "4.000");
    println!("2.000 s = beat {beat:.3}");

    // Loop bar 2, samples 96000..192000. The playhead sits 100 frames
    // before the end, at 191900.
    let mut transport = Transport::with_tempo_map(map);
    let range = LoopRange::new(
        transport
            .tempo_map()
            .beat_to_samples(BeatPosition::new(4.0)),
        transport
            .tempo_map()
            .beat_to_samples(BeatPosition::new(8.0)),
    )?;
    assert_eq!(range.start().samples(), 96_000);
    assert_eq!(range.end().samples(), 192_000);
    transport.set_loop(Some(range));
    transport.set_loop_enabled(true);
    transport.seek(range.end().offset(-100));
    assert_eq!(transport.position().samples(), 191_900);
    transport.play();

    // 100 frames finish the loop, then 156 frames start at sample 96000.
    // The playhead is sample 96156, beat 4.006.
    println!("\nblock of 256 frames across the loop point:");
    let segs: Vec<_> = transport.advance(256).collect();
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].block_offset, 0);
    assert_eq!(segs[0].frames, 100);
    assert_eq!(segs[0].start.samples(), 191_900);
    assert!(!segs[0].wrapped);
    assert_eq!(segs[1].block_offset, 100);
    assert_eq!(segs[1].frames, 156);
    assert_eq!(segs[1].start.samples(), 96_000);
    assert!(segs[1].wrapped);
    assert_eq!(segs[0].frames + segs[1].frames, 256);
    assert_eq!(transport.position().samples(), 96_156);
    assert_eq!(format!("{:.3}", transport.beat_position().beats()), "4.006");
    for seg in &segs {
        println!(
            "  offset {:>3}, {:>3} frames, from sample {}{}",
            seg.block_offset,
            seg.frames,
            seg.start.samples(),
            if seg.wrapped {
                " (wrapped to loop start)"
            } else {
                ""
            }
        );
    }
    println!(
        "playhead now at beat {:.3}",
        transport.beat_position().beats()
    );

    // Stopped, the same 256-frame call is one segment and the playhead
    // stays at sample 96156.
    let at = transport.position().samples();
    transport.stop();
    let segs: Vec<_> = transport.advance(256).collect();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].frames, 256);
    assert_eq!(transport.position().samples(), at);
    println!(
        "stopped: {} segment(s) of {} frames, position unchanged",
        segs.len(),
        segs[0].frames
    );
    Ok(())
}
