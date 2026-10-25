//! An undo stack builds a memory, then three undos and three redos walk it.
//! Saving writes 2927 bytes that reload identical; one more undo is dirty.
//!
//! ```text
//! cargo run --example session_undo
//! ```

use superb::primitives::session::commands::{
    self, AddClip, AddMarker, AddSend, AddTrack, MoveClip, SetTempo, SetTrackOutput,
};
use superb::primitives::session::{
    Clip, ClipSource, Marker, Memory, MidiNote, Track, TrackKind, TrackSend,
};
use superb::primitives::undo::UndoStack;
use superb::primitives::{BeatPosition, Result, SampleRate, TimeDuration, TimePosition};

// One line: name, revision, counts, tempo, end, and the undo/redo labels.
fn describe(memory: &Memory, history: &UndoStack<Memory>) {
    println!(
        "  {:<12} rev {:>3}  tracks {}  clips {}  sends {}  markers {}  tempo {:.0} bpm  end {} smp  | undo: {:?}  redo: {:?}",
        memory.name(),
        memory.revision(),
        memory.track_count(),
        memory.clip_count(),
        memory.send_count(),
        memory.markers().len(),
        memory.tempo_map().tempo_at(BeatPosition::ZERO),
        memory.end_position().samples(),
        history.undo_label().unwrap_or("-"),
        history.redo_label().unwrap_or("-"),
    );
}

fn main() -> Result<()> {
    let rate = SampleRate::HZ_48K;
    assert_eq!(rate.seconds_to_samples(1.0), 48_000);
    let mut memory = Memory::new("untitled", rate);
    let mut history = UndoStack::new();

    println!("edits:");
    // The memory is named "demo memory" and the tempo at beat 0 is 96 bpm.
    history.push(&mut memory, commands::set_session_name("demo memory"))?;
    history.push(&mut memory, SetTempo::new(BeatPosition::ZERO, 96.0))?;
    assert_eq!(memory.name(), "demo memory");
    assert!((memory.tempo_map().tempo_at(BeatPosition::ZERO) - 96.0).abs() < 1e-9);

    // One step, "Add drum track": an audio track coloured 0xE06030 and a
    // 2-second clip (96000 samples). Five applies, so revision 5.
    history.begin_group("Add drum track");
    let drums = memory.allocate_track_id();
    history.push(
        &mut memory,
        AddTrack::new(Track::with_id(drums, "drums", TrackKind::Audio)),
    )?;
    history.push(
        &mut memory,
        commands::set_track_color(drums, Some(0xE0_60_30)),
    )?;
    let loop_id = memory.allocate_clip_id();
    let loop_clip = Clip::with_id(
        loop_id,
        "break.wav",
        TimePosition::ZERO,
        TimeDuration::from_seconds(2.0, rate),
        ClipSource::Audio {
            path: "loops/break.wav".into(),
            offset: TimeDuration::ZERO,
        },
    )?;
    history.push(&mut memory, AddClip::new(drums, loop_clip))?;
    history.end_group();
    assert_eq!(memory.revision(), 5);
    assert_eq!(memory.track_count(), 1);
    assert_eq!(memory.clip_count(), 1);
    assert_eq!(memory.send_count(), 0);
    assert!(memory.markers().is_empty());
    assert_eq!(memory.track(drums)?.name, "drums");
    assert_eq!(memory.track(drums)?.kind, TrackKind::Audio);
    assert_eq!(memory.track(drums)?.color, Some(0xE0_60_30));
    assert_eq!(memory.clip(loop_id)?.length().samples(), 96_000);
    assert_eq!(memory.end_position().samples(), 96_000);
    assert_eq!(history.undo_label(), Some("Add drum track"));
    assert!(history.redo_label().is_none());
    describe(&memory, &history);

    // A MIDI track and a 4-beat clip placed at beat 4. At 96 bpm that is
    // sample 120000, length 120000, so the memory ends at 240000.
    // Notes 60, 64 and 67, velocity 96, channel 0, each 2 beats. Revision 7.
    let keys = memory.allocate_track_id();
    history.push(
        &mut memory,
        AddTrack::new(Track::with_id(keys, "keys", TrackKind::Midi)),
    )?;
    let chord = memory.allocate_clip_id();
    let notes = vec![
        MidiNote {
            beat: 0.0,
            length: 2.0,
            note: 60,
            velocity: 96,
            channel: 0,
        },
        MidiNote {
            beat: 0.0,
            length: 2.0,
            note: 64,
            velocity: 96,
            channel: 0,
        },
        MidiNote {
            beat: 0.0,
            length: 2.0,
            note: 67,
            velocity: 96,
            channel: 0,
        },
    ];
    let bar = memory.tempo_map().beat_to_samples(BeatPosition::new(4.0));
    assert_eq!(bar.samples(), 120_000);
    let chord_clip = Clip::with_id(
        chord,
        "C major",
        bar,
        TimeDuration::from_samples(bar.samples()),
        ClipSource::Midi { notes },
    )?;
    history.push(&mut memory, AddClip::new(keys, chord_clip))?;
    assert_eq!(memory.revision(), 7);
    assert_eq!(memory.track_count(), 2);
    assert_eq!(memory.clip_count(), 2);
    assert_eq!(memory.track(keys)?.kind, TrackKind::Midi);
    assert_eq!(memory.clip(chord)?.start().samples(), 120_000);
    assert_eq!(memory.clip(chord)?.length().samples(), 120_000);
    assert_eq!(memory.end_position().samples(), 240_000);
    let ClipSource::Midi { notes } = &memory.clip(chord)?.source else {
        panic!("chord clip is midi");
    };
    assert_eq!(notes.len(), 3);
    for (note, number) in notes.iter().zip([60_u8, 64, 67]) {
        assert_eq!(note.note, number);
        assert_eq!(note.velocity, 96);
        assert_eq!(note.channel, 0);
        assert!(note.beat.abs() < 1e-9);
        assert!((note.length - 2.0).abs() < 1e-9);
    }
    assert_eq!(history.undo_label(), Some("Add clip"));
    describe(&memory, &history);

    // Keys route to a bus. Drums send to it pre-fader. Levels −3, −6 and
    // −9 coalesce into one "Set send level", left at −9 dB. Revision 13.
    // The end is still 240000.
    let verb = memory.allocate_track_id();
    history.push(
        &mut memory,
        AddTrack::new(Track::with_id(verb, "verb", TrackKind::Bus)),
    )?;
    history.push(&mut memory, SetTrackOutput::new(keys, Some(verb)))?;
    let send = memory.allocate_send_id();
    history.push(
        &mut memory,
        AddSend::new(drums, TrackSend::with_id(send, verb).pre_fader()),
    )?;
    for db in [-3.0, -6.0, -9.0] {
        history.push(&mut memory, commands::set_send_level_db(send, db))?;
    }
    history.seal();
    assert_eq!(memory.revision(), 13);
    assert_eq!(memory.track_count(), 3);
    assert_eq!(memory.send_count(), 1);
    assert_eq!(memory.track(verb)?.kind, TrackKind::Bus);
    assert_eq!(memory.track(keys)?.output(), Some(verb));
    assert!(memory.send(send)?.pre_fader);
    assert_eq!(memory.send(send)?.bus(), verb);
    assert!((memory.send(send)?.state.level_db + 9.0).abs() < 1e-6);
    assert_eq!(memory.end_position().samples(), 240_000);
    assert_eq!(history.undo_label(), Some("Set send level"));
    describe(&memory, &history);

    // Moves to beats 5, 6, 7 and 8 coalesce into one "Move clip". Beat 8
    // is sample 240000; the clip still lasts 120000, so the end is 360000.
    // A marker named "verse" sits at sample 120000. Revision 18.
    for beat in [5.0, 6.0, 7.0, 8.0] {
        let pos = memory.tempo_map().beat_to_samples(BeatPosition::new(beat));
        history.push(&mut memory, MoveClip::new(chord, pos))?;
    }
    history.seal();
    assert_eq!(
        memory
            .tempo_map()
            .beat_to_samples(BeatPosition::new(8.0))
            .samples(),
        240_000
    );
    let marker = memory.allocate_marker_id();
    history.push(
        &mut memory,
        AddMarker::new(Marker::with_id(marker, "verse", bar)),
    )?;
    assert_eq!(memory.revision(), 18);
    assert_eq!(memory.markers().len(), 1);
    assert_eq!(memory.markers()[0].name, "verse");
    assert_eq!(memory.markers()[0].position().samples(), 120_000);
    assert_eq!(memory.clip(chord)?.start().samples(), 240_000);
    assert_eq!(memory.end_position().samples(), 360_000);
    assert_eq!(history.undo_label(), Some("Add marker"));
    describe(&memory, &history);
    // Eleven steps. Most recent first. The level drag and the move drag
    // are one step each, so eighteen applies became eleven steps.
    let labels: Vec<_> = history.undo_labels().collect();
    assert_eq!(history.undo_len(), 11);
    assert_eq!(
        labels,
        [
            "Add marker",
            "Move clip",
            "Set send level",
            "Add send",
            "Route track",
            "Add track",
            "Add clip",
            "Add track",
            "Add drum track",
            "Set tempo",
            "Rename session",
        ]
    );
    println!(
        "  history ({} steps): {:?}",
        history.undo_len(),
        history.undo_labels().collect::<Vec<_>>()
    );

    // Undo drops the marker (revision 19, end still 360000), then the move
    // (clip back at 120000, end 240000, revision 20), then the send level
    // (back to 0 dB, revision 21).
    println!("undo x3:");
    for step in 0..3 {
        history.undo(&mut memory)?;
        describe(&memory, &history);
        match step {
            0 => {
                assert!(memory.markers().is_empty());
                assert_eq!(memory.end_position().samples(), 360_000);
                assert_eq!(memory.revision(), 19);
                assert_eq!(history.undo_label(), Some("Move clip"));
                assert_eq!(history.redo_label(), Some("Add marker"));
            }
            1 => {
                assert_eq!(memory.clip(chord)?.start().samples(), 120_000);
                assert_eq!(memory.end_position().samples(), 240_000);
                assert_eq!(memory.revision(), 20);
                assert!((memory.send(send)?.state.level_db + 9.0).abs() < 1e-6);
                assert_eq!(history.undo_label(), Some("Set send level"));
                assert_eq!(history.redo_label(), Some("Move clip"));
            }
            _ => {
                assert!(memory.send(send)?.state.level_db.abs() < 1e-6);
                assert_eq!(memory.end_position().samples(), 240_000);
                assert_eq!(memory.revision(), 21);
                assert_eq!(history.undo_label(), Some("Add send"));
                assert_eq!(history.redo_label(), Some("Set send level"));
            }
        }
    }
    // Redo restores −9 dB, the clip end at 360000, and the marker.
    // Revision 24, and the redo stack is empty.
    println!("redo x3:");
    let mut redos = 0;
    while history.redo(&mut memory)? {
        describe(&memory, &history);
        redos += 1;
    }
    assert_eq!(redos, 3);
    assert_eq!(memory.revision(), 24);
    assert!((memory.send(send)?.state.level_db + 9.0).abs() < 1e-6);
    assert_eq!(memory.clip(chord)?.start().samples(), 240_000);
    assert_eq!(memory.end_position().samples(), 360_000);
    assert_eq!(memory.markers().len(), 1);
    assert!(history.redo_label().is_none());
    assert_eq!(history.undo_label(), Some("Add marker"));

    // 2927 bytes, the reload matches, clean until one more undo.
    let path = "session_undo.superb.json";
    memory.save(path)?;
    history.mark_clean();
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let reloaded = Memory::load(path)?;
    let identical = reloaded.snapshot() == memory.snapshot();
    assert_eq!(bytes, 2927);
    assert!(identical);
    assert!(history.is_clean());
    println!(
        "saved {path} ({} bytes) and reloaded: identical = {}",
        bytes, identical
    );
    println!("clean after save: {}", history.is_clean());
    history.undo(&mut memory)?;
    assert!(!history.is_clean());
    println!("clean after one more undo: {}", history.is_clean());
    Ok(())
}
