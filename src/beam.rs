//! Code 6 beam connection (`AUTO` / `ADJ`).
//!
//! A beam is placed as an independent object first.  The original workflow is
//! `NEW ITEM → Create Code 6 Beam`, drag the beam over the notes, then press
//! `AUTO` in the beam menu.  AUTO finds the eighth-or-shorter notes under the
//! beam, aligns the endpoints with their stems, derives the number of beams
//! from their flags and sets the stem direction.

use crate::model::{Item, Score};

#[derive(Debug, Clone)]
pub struct BeamPlan {
    pub beam: usize,
    pub notes: usize,
    pub first_x: f32,
    pub last_x: f32,
    pub base_level: f32,
    pub count: i32,
    pub direction: i32,
}

/// Build a connection plan without changing the score.  `opposite` requests
/// the opposite stem direction (the `AUTOX` command); `direction` forces a
/// particular direction (the `STEM UP/DOWN` buttons).
pub fn plan(
    score: &Score,
    beam: usize,
    opposite: bool,
    direction: Option<i32>,
) -> Result<BeamPlan, String> {
    let item = score
        .items
        .get(beam)
        .filter(|i| i.code == 6)
        .ok_or_else(|| "AUTO: select a Code 6 beam first.".to_string())?;
    let staff = item.staff();
    let current_dir = (item.get(7).abs().round() as i32 / 10).clamp(1, 2);
    let want = direction.unwrap_or_else(|| {
        if opposite {
            3 - current_dir
        } else {
            current_dir
        }
    });
    let want = want.clamp(1, 2);
    let (x1, x2) = (item.get(3), item.get(6));
    let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
    let lo = lo - 0.75;
    let hi = hi + 0.75;

    let mut candidates: Vec<(usize, f32, f32)> = Vec::new();
    for (ni, note) in score.items.iter().enumerate() {
        if note.code != 1 || note.staff() != staff {
            continue;
        }
        let x = note.get(3);
        if x < lo || x > hi {
            continue;
        }
        if (note.get(7).abs() - 65.0).abs() < 0.5 {
            continue;
        }
        let stem = (note.get(5).abs().round() as i32 / 10) % 10;
        if stem != want {
            continue;
        }
        if flag_count(note) == 0 {
            continue;
        }
        let level = note.get(4);
        if level.abs() >= 100.0 {
            continue;
        }
        if candidates
            .last()
            .map(|(_, lx, _)| (x - *lx).abs() > 0.05)
            .unwrap_or(true)
        {
            candidates.push((ni, x, level));
        }
    }
    if candidates.len() < 2 {
        let name = if want == 1 {
            "up-stemmed"
        } else {
            "down-stemmed"
        };
        return Err(format!(
            "AUTO: need at least two {name} eighth-or-shorter notes under the beam."
        ));
    }
    candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let first = *candidates.first().unwrap();
    let last = *candidates.last().unwrap();
    let count = candidates
        .iter()
        .map(|(ni, _, _)| flag_count(&score.items[*ni]))
        .max()
        .unwrap_or(1)
        .clamp(1, 6);
    let base_level = if want == 1 {
        candidates
            .iter()
            .map(|(_, _, lv)| *lv)
            .fold(f32::NEG_INFINITY, f32::max)
    } else {
        candidates
            .iter()
            .map(|(_, _, lv)| *lv)
            .fold(f32::INFINITY, f32::min)
    };
    Ok(BeamPlan {
        beam,
        notes: candidates.len(),
        first_x: first.1,
        last_x: last.1,
        base_level,
        count,
        direction: want,
    })
}

/// Apply a plan produced by [`plan`].
pub fn apply(score: &mut Score, plan: &BeamPlan) {
    if let Some(item) = score.items.get_mut(plan.beam) {
        item.set(3, plan.first_x);
        item.set(6, plan.last_x);
        item.set(4, plan.base_level);
        item.set(5, plan.base_level);
        item.set(7, (plan.direction * 10 + plan.count) as f32);
        item.set(9, -1.0);
        item.set(30, 1.0);
    }
}

/// Number of flags/beams implied by a note.  P9's unit digit overrides the
/// duration-derived value, matching the engraver.
pub fn flag_count(item: &Item) -> i32 {
    let p9 = item.get(9).round() as i32;
    let explicit = p9.abs() % 10;
    if explicit > 0 {
        return explicit;
    }
    match item.get(7) {
        d if d <= 0.07 => 4,
        d if d <= 0.13 => 3,
        d if d <= 0.26 => 2,
        d if d <= 0.51 => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score_with_notes(levels: &[f32], stem: f32, dur: f32) -> Score {
        let mut score = Score::default();
        score
            .items
            .push(Item::new(8, vec![8.0, 1.0, 0.0, 0.0, 1.0, 200.0, 5.0]));
        for (i, level) in levels.iter().enumerate() {
            score.items.push(Item::new(
                1,
                vec![1.0, 1.0, 10.0 + i as f32 * 5.0, *level, stem, 0.0, dur],
            ));
        }
        let mut beam = Item::new(6, vec![6.0, 1.0, 8.0, 13.0, 13.0, 32.0, 11.0]);
        beam.set(30, 2.0);
        score.items.push(beam);
        score
    }

    #[test]
    fn auto_connects_up_stem_notes() {
        let mut score = score_with_notes(&[3.0, 5.0, 7.0, 6.0], 10.0, 0.5);
        let beam = score.items.len() - 1;
        let plan = plan(&score, beam, false, None).unwrap();
        assert_eq!(plan.notes, 4);
        assert_eq!(plan.direction, 1);
        assert_eq!(plan.count, 1);
        assert_eq!(plan.base_level, 7.0);
        apply(&mut score, &plan);
        let b = &score.items[beam];
        assert_eq!(b.get(3), 10.0);
        assert_eq!(b.get(6), 25.0);
        assert_eq!(b.get(4), 7.0);
        assert_eq!(b.get(7), 11.0);
        assert_eq!(b.get(9), -1.0);
        assert_eq!(b.get(30), 1.0);
    }

    #[test]
    fn auto_uses_shortest_note_for_beam_count() {
        let mut score = score_with_notes(&[3.0, 5.0, 7.0], 20.0, 0.25);
        let beam = score.items.len() - 1;
        score.items[beam].set(7, 21.0);
        let plan = plan(&score, beam, false, None).unwrap();
        assert_eq!(plan.direction, 2);
        assert_eq!(plan.count, 2);
        apply(&mut score, &plan);
        assert_eq!(score.items[beam].get(7), 22.0);
        assert_eq!(score.items[beam].get(4), 3.0);
    }

    #[test]
    fn auto_refuses_a_single_note() {
        let mut score = score_with_notes(&[3.0, 5.0], 10.0, 0.5);
        let beam = score.items.len() - 1;
        score.items[beam].set(6, 10.0);
        assert!(plan(&score, beam, false, None).is_err());
    }
}
