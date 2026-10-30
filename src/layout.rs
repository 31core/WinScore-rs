//! Horizontal spacing, Lineup (`LI`) and Justify (`J`/`LJ`).
//!
//! WinScore stores already-engraved positions in P3, so loading and saving a
//! score never changes its spacing.  The original editor exposed a separate
//! layout pass for newly entered or edited measures.  This module implements
//! the documented ideal-width model:
//!
//! * a note/rest advances by `20 * |P7| * P5` horizontal units;
//! * a chord shares one horizontal slot;
//! * barlines reserve a small fixed slot;
//! * Lineup places the resulting columns from left to right;
//! * Justify distributes the remaining width over the gaps, and reports the
//!   historical warning when the requested range is too narrow.
//!
//! The imported sample scores deliberately keep their source P3 values until
//! the user invokes `LI`, `J`, `LJ` or `LJA`.

use crate::model::Score;

pub const QUARTER_WIDTH: f32 = 20.0;
pub const MIN_GAP: f32 = 0.2;

#[derive(Debug, Clone)]
pub struct LayoutReport {
    pub staves: Vec<i32>,
    pub moved: usize,
    pub warnings: Vec<String>,
}

impl LayoutReport {
    fn new() -> Self {
        Self {
            staves: Vec::new(),
            moved: 0,
            warnings: Vec::new(),
        }
    }

    pub fn summary(&self) -> String {
        if self.moved == 0 {
            return "Layout: no notes or rests to place.".into();
        }
        let mut s = format!(
            "Layout: {} object{} on staff{} {}.",
            self.moved,
            if self.moved == 1 { "" } else { "s" },
            if self.staves.len() == 1 { "" } else { "s" },
            self.staves
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        if !self.warnings.is_empty() {
            s.push(' ');
            s.push_str(&self.warnings.join(" "));
        }
        s
    }
}

#[derive(Debug, Clone)]
struct Event {
    members: Vec<usize>,
    old_x: f32,
    width: f32,
    barline: bool,
}

/// Lay out one staff.  `left`/`right` are SCORE horizontal units.  When
/// `justify` is false this is a pure Lineup pass; when true the columns are
/// spread across the full range.
pub fn layout_staff(
    score: &mut Score,
    staff: i32,
    left: f32,
    right: f32,
    justify: bool,
) -> (usize, Vec<String>) {
    let (staff_left, staff_right, staff_size) = staff_geometry(score, staff);
    let left = if left.is_finite() { left } else { staff_left };
    let right = if right.is_finite() {
        right
    } else {
        staff_right
    };
    let right = right.max(left + 1.0);
    let size = if staff_size.abs() < 0.001 {
        1.0
    } else {
        staff_size.abs()
    };

    resolve_chord_p10(score, staff);
    let mut events = collect_events(score, staff, left, size);
    if events.is_empty() {
        return (0, Vec::new());
    }
    events.sort_by(|a, b| {
        a.old_x
            .partial_cmp(&b.old_x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Keep a clef/key/meter at the start of the staff from being overrun.
    let mut start = left;
    for item in score.items.iter().filter(|i| i.staff() == staff) {
        if !matches!(item.code, 3 | 17 | 18) {
            continue;
        }
        let x = item.get(3);
        if x > events[0].old_x + 0.5 {
            continue;
        }
        let slot = match item.code {
            3 => 6.5 * size,
            18 => 9.0 * size,
            17 => (item.get(5).abs() * 2.8 + 0.5).max(2.5) * size,
            _ => 0.0,
        };
        start = start.max(left + x + slot);
    }

    let mut widths: Vec<f32> = events.iter().map(|e| e.width).collect();
    let needed: f32 = widths.iter().sum();
    let available = (right - start).max(0.0);
    let mut warnings = Vec::new();
    let mut gap = 0.0;
    if justify {
        if needed > available + 0.001 {
            warnings.push(format!(
                "*** Not enough space to justify ideally. Needed= {:.2}, Available= {:.2}.",
                needed, available
            ));
            if events.len() > 1 {
                // Section 4.5.1: reduce every slot by the same amount rather
                // than scaling proportionally.  This keeps long and short
                // values in their original relationship while guaranteeing the
                // right edge.
                let deficit = (needed - available) / (events.len() - 1) as f32;
                for width in &mut widths {
                    *width = (*width - deficit).max(0.1);
                }
            }
            gap = 0.0;
        } else if events.len() > 1 {
            gap = (available - needed) / (events.len() - 1) as f32;
            if gap < 0.0 {
                gap = 0.0;
            }
        }
    }

    let mut x = start;
    let mut moved = 0usize;
    for (event, width) in events.iter().zip(widths.iter()) {
        let anchor = event.old_x;
        for &idx in &event.members {
            let item = &mut score.items[idx];
            let offset = item.get(3) - anchor;
            let new_x = if event.barline {
                // Barlines are vertical rules, so any small per-staff offset
                // must be preserved exactly.
                x + offset
            } else {
                x + offset
            };
            if (item.get(3) - new_x).abs() > 0.0005 {
                item.set(3, new_x);
            }
            if !event.barline {
                moved += 1;
            }
        }
        x += *width + gap;
    }

    // Beam endpoints follow the notes after a layout pass, as specified in
    // section 4.8.  Slurs and endings are intentionally left alone (the
    // original editor asks for `CRV`/manual adjustment after `LAY`).
    retarget_beams(score, staff, &events);

    (moved, warnings)
}

/// Lay out all staves in the inclusive range.  Each staff is processed
/// independently; this is the useful single-page behaviour and matches the
/// original `LI` selection prompt when an explicit staff range is supplied.
pub fn layout_score(
    score: &mut Score,
    low_staff: i32,
    high_staff: i32,
    left: Option<f32>,
    right: Option<f32>,
    justify: bool,
) -> LayoutReport {
    let mut report = LayoutReport::new();
    let mut staffs: Vec<i32> = score
        .items
        .iter()
        .filter(|i| i.staff() >= low_staff && i.staff() <= high_staff)
        .map(|i| i.staff())
        .filter(|n| (1..=256).contains(n))
        .collect();
    staffs.sort_unstable();
    staffs.dedup();
    for staff in staffs {
        let (default_left, default_right, _) = staff_geometry(score, staff);
        let (n, warnings) = layout_staff(
            score,
            staff,
            left.unwrap_or(default_left),
            right.unwrap_or(default_right),
            justify,
        );
        if n > 0 {
            report.staves.push(staff);
            report.moved += n;
        }
        report.warnings.extend(warnings);
    }
    report
}

/// SCORE resolves seconds/unisons in the same column by moving one head with
/// P10 rather than changing P3.  This pass only fills P10 when the user has not
/// already provided an offset.
fn resolve_chord_p10(score: &mut Score, staff: i32) {
    let mut columns: Vec<(f32, Vec<usize>)> = Vec::new();
    for (idx, item) in score.items.iter().enumerate() {
        if item.code != 1 || item.staff() != staff {
            continue;
        }
        if (item.get(7).abs() - 65.0).abs() < 0.5 {
            continue;
        }
        let x = item.get(3);
        if let Some((_, notes)) = columns.iter_mut().find(|(cx, _)| (x - *cx).abs() < 0.05) {
            notes.push(idx);
        } else {
            columns.push((x, vec![idx]));
        }
    }
    for (_, mut notes) in columns {
        if notes.len() < 2 {
            continue;
        }
        notes.sort_by(|a, b| {
            score.items[*a]
                .get(4)
                .partial_cmp(&score.items[*b].get(4))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut last_level: Option<f32> = None;
        for idx in notes {
            let level = score.items[idx].get(4);
            let p10 = score.items[idx].get(10);
            if last_level
                .map(|prev| (level - prev).abs() <= 1.01)
                .unwrap_or(false)
                && p10.abs() < 0.01
            {
                score.items[idx].set(10, 10.0);
            }
            last_level = Some(level);
        }
    }
}

fn staff_geometry(score: &Score, staff: i32) -> (f32, f32, f32) {
    for item in score.items.iter().filter(|i| i.code == 8) {
        if item.staff() != staff {
            continue;
        }
        let left = item.get(3);
        let mut right = item.get(6);
        if right.abs() < 0.001 {
            right = 200.0;
        }
        let size = {
            let v = item.get(5);
            if v.abs() < 0.001 { 1.0 } else { v.abs() }
        };
        return (left, right, size);
    }
    (0.0, 200.0, 1.0)
}

fn collect_events(score: &Score, staff: i32, left: f32, size: f32) -> Vec<Event> {
    // Both kinds share one horizontal stream.  Keeping barlines in the same
    // ordered list prevents notes from being laid out past a barline that was
    // originally between them.
    let mut raw: Vec<(usize, f32, bool)> = Vec::new();
    for (idx, item) in score.items.iter().enumerate() {
        if item.staff() != staff {
            continue;
        }
        match item.code {
            1 | 2 => {
                let p7 = item.get(7).abs();
                // P7=65 is SCORE's "grace note, do not space" marker.
                if (p7 - 65.0).abs() < 0.5 {
                    continue;
                }
                raw.push((idx, item.get(3), false));
            }
            14 => raw.push((idx, item.get(3), true)),
            _ => {}
        }
    }
    raw.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut out: Vec<Event> = Vec::new();
    for (idx, x, barline) in raw {
        if let Some(last) = out.last_mut()
            && last.barline == barline
            && (last.old_x - x).abs() < 0.35
        {
            last.members.push(idx);
            continue;
        }
        out.push(Event {
            members: vec![idx],
            old_x: x,
            width: if barline { 3.5 * size } else { 0.0 },
            barline,
        });
    }

    for event in &mut out {
        if event.barline {
            continue;
        }
        // A chord advances by the duration of its stemmed member.  If no
        // member has a stem (a whole-note chord), use the longest duration.
        let mut rhythm: Option<f32> = None;
        for &idx in &event.members {
            let item = &score.items[idx];
            let p7 = item.get(7).abs();
            let p5 = item.get(5).abs().round() as i32;
            let stem = (p5 / 10) % 10;
            let candidate = if (item.code == 1 && (stem == 1 || stem == 2)) || item.code == 2 {
                Some(p7)
            } else {
                None
            };
            if let Some(c) = candidate {
                rhythm = Some(rhythm.map_or(c, |r: f32| r.max(c)));
            }
        }
        let rhythm = rhythm.unwrap_or_else(|| {
            event
                .members
                .iter()
                .map(|&i| score.items[i].get(7).abs())
                .fold(0.0, f32::max)
        });
        let rhythm_width = (QUARTER_WIDTH * rhythm * size).max(0.0);
        // 20*P7 is only the rhythmic advance.  The collision box also has to
        // hold the head, accidental and augmentation dots; otherwise dense
        // 16th/32nd runs and accidentals overlap after Justify.
        let space_units = 6.3 * size / 2.7;
        let mut visual = 0.0_f32;
        for &idx in &event.members {
            let item = &score.items[idx];
            if item.code != 1 {
                continue;
            }
            let head_code = item.get(6).round() as i32;
            let note_size = {
                let v = item.get(15).abs();
                if v < 0.001 {
                    1.0
                } else {
                    if item.get(4).abs() >= 100.0 {
                        v * 0.58
                    } else {
                        v
                    }
                }
            };
            let head_w = if matches!(head_code % 10, 2 | 3) {
                1.69
            } else {
                1.18
            } * space_units
                * note_size;
            let acc = ((item.get(5).abs().round() as i32) % 10) != 0;
            let acc_size = {
                let p29 = item.get(29);
                if p29.abs() < 0.001 { 1.0 } else { p29.abs() }
            };
            let acc_w = if acc {
                1.15 * space_units * acc_size
            } else {
                0.0
            };
            let p9 = item.get(9).round() as i32;
            let dots = ((p9 / 10).abs() % 10) as f32;
            let dot_size = {
                let p31 = item.get(31);
                if p31.abs() < 0.0001 { 1.0 } else { p31.abs() }
            };
            let dot_w = dots * 0.55 * space_units * dot_size;
            let p10 = item.get(10).abs();
            let p10_w = ((p10 % 100.0) / 10.0) * head_w;
            visual = visual.max(head_w + acc_w + dot_w + p10_w);
        }
        event.width = rhythm_width.max(visual);
    }
    // A barline before the first note should not be counted as an extra
    // leading slot; it already sits at the start of the staff.
    if out.first().map(|e| e.barline).unwrap_or(false) && out[0].old_x <= left + 0.5 {
        out.remove(0);
    }
    out
}

fn retarget_beams(score: &mut Score, staff: i32, events: &[Event]) {
    let mut note_positions: Vec<(usize, f32)> = Vec::new();
    for event in events {
        if event.barline {
            continue;
        }
        for &idx in &event.members {
            note_positions.push((idx, score.items[idx].get(3)));
        }
    }
    for idx in 0..score.items.len() {
        let beam = &score.items[idx];
        if beam.code != 6 || beam.staff() != staff {
            continue;
        }
        let x1 = beam.get(3);
        let x2 = beam.get(6);
        let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        let mut members: Vec<f32> = note_positions
            .iter()
            .filter(|(_, x)| *x >= lo - 0.35 && *x <= hi + 0.35)
            .map(|(_, x)| *x)
            .collect();
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let first = members[0];
        let last = *members.last().unwrap();
        let (new1, new2) = if x1 <= x2 {
            (first, last)
        } else {
            (last, first)
        };
        let beam = &mut score.items[idx];
        beam.set(3, new1);
        beam.set(6, new2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;

    fn staff() -> Item {
        Item::new(8, vec![8.0, 1.0, 0.0, 0.0, 1.0, 200.0, 5.0])
    }

    fn note(x: f32, p7: f32) -> Item {
        Item::new(1, vec![1.0, 1.0, x, 3.0, 10.0, 0.0, p7])
    }

    #[test]
    fn lineup_places_quarters_by_rhythm_width() {
        let mut score = Score::default();
        score.items.push(staff());
        for x in [0.0, 20.0, 40.0, 60.0] {
            score.items.push(note(x, 1.0));
        }
        let report = layout_score(&mut score, 1, 1, None, None, false);
        assert_eq!(report.moved, 4);
        let xs: Vec<f32> = score.items[1..].iter().map(|i| i.p3()).collect();
        assert_eq!(xs, vec![0.0, 20.0, 40.0, 60.0]);
    }

    #[test]
    fn justify_fills_the_requested_range() {
        let mut score = Score::default();
        score.items.push(staff());
        for x in [0.0, 20.0, 40.0, 60.0] {
            score.items.push(note(x, 1.0));
        }
        layout_score(&mut score, 1, 1, None, None, true);
        let xs: Vec<f32> = score.items[1..].iter().map(|i| i.p3()).collect();
        assert_eq!(xs, vec![0.0, 60.0, 120.0, 180.0]);
    }

    #[test]
    fn narrow_range_reports_and_avoids_overlap() {
        let mut score = Score::default();
        score
            .items
            .push(Item::new(8, vec![8.0, 1.0, 0.0, 0.0, 1.0, 70.0, 5.0]));
        for x in [0.0, 20.0, 40.0, 60.0] {
            score.items.push(note(x, 1.0));
        }
        let report = layout_score(&mut score, 1, 1, None, None, true);
        assert_eq!(report.moved, 4);
        assert!(report.summary().contains("Not enough space"));
        for w in score.items[1..].windows(2) {
            assert!(w[1].p3() >= w[0].p3());
        }
        assert!(score.items.last().unwrap().p3() <= 70.0);
    }

    #[test]
    fn dense_accidentals_expand_the_collision_box() {
        let mut score = Score::default();
        score.items.push(staff());
        for x in [0.0, 10.0, 20.0] {
            score
                .items
                .push(Item::new(1, vec![1.0, 1.0, x, 3.0, 12.0, 0.0, 0.0625]));
        }
        layout_score(&mut score, 1, 1, None, None, false);
        let xs: Vec<f32> = score.items[1..].iter().map(|i| i.p3()).collect();
        assert!(
            xs[1] - xs[0] > 4.0,
            "3.5-unit head+accidental box expected, got {xs:?}"
        );
        assert!(xs[2] - xs[1] > 4.0);
    }

    #[test]
    fn seconds_get_a_p10_offset() {
        let mut score = Score::default();
        score.items.push(staff());
        score.items.push(note(10.0, 1.0));
        score
            .items
            .push(Item::new(1, vec![1.0, 1.0, 10.0, 4.0, 10.0, 0.0, 1.0]));
        layout_score(&mut score, 1, 1, None, None, false);
        assert_eq!(score.items[2].get(10), 10.0);
    }

    #[test]
    fn chord_members_share_a_slot() {
        let mut score = Score::default();
        score.items.push(staff());
        score.items.push(note(10.0, 1.0));
        score
            .items
            .push(Item::new(1, vec![1.0, 1.0, 10.0, 7.0, 0.0, 0.0, 1.0]));
        score.items.push(note(30.0, 1.0));
        layout_score(&mut score, 1, 1, None, None, false);
        assert_eq!(score.items[1].p3(), score.items[2].p3());
        assert_eq!(score.items[3].p3(), 20.0);
    }
}
