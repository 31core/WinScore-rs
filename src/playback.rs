//! A small built-in audition synthesiser.
//!
//! WinScore used MIDI; for a self-contained desktop build we render the
//! engraved notes to a temporary WAV and hand it to the system player.  The
//! purpose is musical orientation while editing, not concert-hall fidelity.

use std::path::PathBuf;

use crate::model::{Item, Score};

#[derive(Debug, Clone, Copy)]
struct Event {
    start: f32,
    dur: f32,
    midi: i32,
    velocity: f32,
}

pub fn export_wav(score: &Score, path: &PathBuf) -> anyhow::Result<f32> {
    const RATE: u32 = 44_100;
    let events = collect_events(score);
    if events.is_empty() {
        anyhow::bail!("no sounding notes in this score");
    }
    let total = events
        .iter()
        .map(|e| e.start + e.dur + 0.3)
        .fold(0.0f32, f32::max)
        .min(120.0);
    let frames = (total * RATE as f32).ceil() as usize + RATE as usize;
    let mut mix = vec![0.0f32; frames];
    for ev in &events {
        let start = (ev.start * RATE as f32) as usize;
        let len = ((ev.dur * RATE as f32) as usize).clamp(1, RATE as usize * 8);
        let freq = 440.0 * 2f32.powf((ev.midi as f32 - 69.0) / 12.0);
        for i in 0..len {
            let t = i as f32 / RATE as f32;
            let env = envelope(t, ev.dur);
            let phase = std::f32::consts::TAU * freq * t;
            let s = phase.sin() * 0.72
                + (phase * 2.0).sin() * 0.18
                + (phase * 3.0).sin() * 0.07
                + (phase * 4.01).sin() * 0.03;
            if start + i < mix.len() {
                mix[start + i] += s * env * ev.velocity * 0.11;
            }
        }
    }
    // Gentle limiter, then 16-bit PCM.
    let mut bytes = Vec::with_capacity(44 + frames * 2);
    let data_len = frames * 2;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in mix {
        let s = (s * 1.6).tanh().clamp(-1.0, 1.0);
        let v = (s * 32767.0) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, bytes)?;
    Ok(total)
}

pub fn play(score: &Score) -> anyhow::Result<String> {
    let path = std::env::temp_dir().join("winscore-audition.wav");
    let secs = export_wav(score, &path)?;
    std::process::Command::new("afplay")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(format!(
        "Auditioning {:.1} seconds of music with the built-in synthesiser.",
        secs
    ))
}

fn envelope(t: f32, dur: f32) -> f32 {
    let attack = 0.012f32;
    let release = 0.12f32;
    let a = (t / attack).clamp(0.0, 1.0);
    let r = if t > (dur - release).max(0.0) {
        ((dur - t) / release).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let decay = (1.0 - 0.22 * (t / dur.max(0.001))).clamp(0.3, 1.0);
    a * r * decay
}

fn collect_events(score: &Score) -> Vec<Event> {
    // Find the first clef and key signature on each staff.
    let mut clef: std::collections::BTreeMap<i32, i32> = std::collections::BTreeMap::new();
    let mut keysig: std::collections::BTreeMap<i32, i32> = std::collections::BTreeMap::new();
    for it in &score.items {
        let staff = it.staff();
        if it.code == 3 {
            let p5 = it.get(5).round() as i32;
            clef.entry(staff).or_insert(match p5 {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                _ => 0,
            });
        } else if it.code == 17 {
            keysig.entry(staff).or_insert(it.get(5).round() as i32);
        }
    }

    let mut events = Vec::new();
    let mut staffs: Vec<i32> = score.staff_ids();
    if staffs.is_empty() {
        staffs.push(1);
    }
    for staff in staffs {
        if clef.get(&staff).copied().unwrap_or(0) == 4 {
            continue;
        }
        let mut items: Vec<&Item> = score
            .items
            .iter()
            .filter(|i| i.staff() == staff && (i.code == 1 || i.code == 2))
            .collect();
        items.sort_by(|a, b| {
            a.p3()
                .partial_cmp(&b.p3())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut time = 0.0f32;
        let mut last_x = f32::NEG_INFINITY;
        let mut chord_start = 0.0f32;
        for it in items {
            let x = it.p3();
            let dur_q = it.get(7).abs().max(0.03);
            let is_new = x > last_x + 0.001;
            if is_new {
                time += 0.0;
                chord_start = time;
            }
            if it.code == 1 {
                if let Some(midi) = note_midi(
                    it,
                    clef.get(&staff).copied().unwrap_or(0),
                    keysig.get(&staff).copied().unwrap_or(0),
                ) {
                    events.push(Event {
                        start: chord_start * 0.5,
                        dur: (dur_q * 0.48).clamp(0.08, 4.0),
                        midi,
                        velocity: 0.9,
                    });
                }
            }
            if is_new {
                // Advance the clock after handing out all notes of a chord;
                // chords share a start position, so we use the maximum duration
                // in the chord as the next increment.
                time += dur_q;
                last_x = x;
            }
        }
    }
    events
}

fn note_midi(item: &Item, clef: i32, keysig: i32) -> Option<i32> {
    let mut level = item.get(4);
    if level >= 100.0 {
        level -= 100.0;
    } else if level <= -100.0 {
        level += 100.0;
    }
    let base = match clef {
        1 => 13.0,
        2 => 7.0,
        3 => 9.0,
        _ => 1.0,
    };
    let diatonic = (level - base).round() as i32;
    let idx = diatonic.rem_euclid(7) as usize;
    let octave = 4 + diatonic.div_euclid(7);
    let semis = [0, 2, 4, 5, 7, 9, 11][idx];
    let mut midi = 12 * (octave + 1) + semis;
    let acc = item.get(5).round() as i32 % 10;
    midi += match acc {
        1 => -1,
        2 => 1,
        3 => 0,
        4 => -2,
        5 => 2,
        6 => -1,
        7 => -1,
        8 => 1,
        9 => 1,
        _ => {
            // Apply the written key signature to unmarked notes.
            let key = keysig.signum();
            if key != 0 {
                let order = [3usize, 0, 4, 1, 5, 2, 6]; // F C G D A E B
                if order[..keysig.unsigned_abs().min(7) as usize].contains(&idx) {
                    key
                } else {
                    0
                }
            } else {
                0
            }
        }
    };
    Some(midi)
}
