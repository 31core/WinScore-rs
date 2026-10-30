//! The engraver: turns SCORE object parameters into a resolution-independent
//! page of vector artwork.
//!
//! The coordinate system is measured in PostScript points (1/72 inch), with
//! the origin at the top-left of a US Letter page.  SCORE's horizontal units
//! run 0..200 across 7.5 inches (2.7 points per unit), and its vertical unit is
//! one diatonic step (3.15 points at staff size 1).

use std::collections::BTreeMap;

use crate::draw::{Color, DrawList, Path, PathCmd, Pt};
use crate::font::*;
use crate::model::{Item, Score};

// WinScore's engraved page is 554 x 643 pt (7.69 x 8.93 in), not US Letter.
// These measurements come directly from the reference PDF written by
// WINSCORE 5.00; the music area is exactly 540 pt (200 SCORE units) wide.
pub const PAGE_W: f32 = 554.0;
pub const PAGE_H: f32 = 643.0;
pub const PAGE_LEFT: f32 = 11.8;
pub const PAGE_BOTTOM: f32 = 31.46;
pub const SCORE_UNIT: f32 = 2.7;
pub const STEP: f32 = 3.15;
pub const STAFF_DEFAULT_GAP: f32 = 9.0 * 6.3;
pub const STAFF_SPACE: f32 = 6.3;

/// The .mus format does not store a paper size.  WinScore's normal page is
/// 554 x 643 pt; scores whose uppermost staff or title needs more room (the
/// Leland Smith sample, for example) were printed on US Letter instead.
pub fn page_size_for(score: &Score) -> (f32, f32) {
    for item in score.items.iter().filter(|i| i.code == 8) {
        let id = item.staff();
        if !(1..=256).contains(&id) {
            continue;
        }
        let size = {
            let v = item.get(5);
            if v.abs() < 0.001 { 1.0 } else { v.abs() }
        };
        let space = STAFF_SPACE * size;
        let p10 = item.get(10);
        let unit = if score.trailer.metric {
            72.0 / 2.54
        } else {
            72.0
        };
        let bottom = if p10.abs() > 0.0001 {
            PAGE_BOTTOM + p10 * unit
        } else {
            PAGE_BOTTOM + (id - 1) as f32 * STAFF_DEFAULT_GAP + item.get(4) * size * STEP
        };
        if bottom + 4.0 * space + 6.0 > PAGE_H {
            return (612.0, 792.0);
        }
    }
    (PAGE_W, PAGE_H)
}

#[derive(Debug, Clone)]
pub struct StaffGeo {
    pub id: i32,
    pub x0: f32,
    pub x1: f32,
    pub bottom: f32,
    pub top: f32,
    pub size: f32,
    pub space: f32,
    pub lines: i32,
    pub visible: bool,
    pub bottom_level: f32,
    pub line_width: f32,
}

impl StaffGeo {
    pub fn level_y(&self, level: f32) -> f32 {
        self.bottom - (level - self.bottom_level) * self.space * 0.5
    }
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub item: usize,
    pub min: Pt,
    pub max: Pt,
}

impl Hit {
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.min.x - 2.0
            && p.x <= self.max.x + 2.0
            && p.y >= self.min.y - 2.0
            && p.y <= self.max.y + 2.0
    }
    pub fn center(&self) -> Pt {
        Pt::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }
}

#[derive(Debug, Clone)]
pub struct EngravedPage {
    pub draws: DrawList,
    pub hits: Vec<Hit>,
    pub staves: BTreeMap<i32, StaffGeo>,
    pub width: f32,
    pub height: f32,
}

pub struct Engraver<'a> {
    score: &'a Score,
    fonts: &'a FontBook,
    draws: DrawList,
    hits: Vec<Hit>,
    staves: BTreeMap<i32, StaffGeo>,
    beams: Vec<BeamInfo>,
    beam_for_note: BTreeMap<usize, (usize, f32)>,
    page_w: f32,
    page_h: f32,
}

#[derive(Debug, Clone)]
struct BeamInfo {
    item: usize,
    staff: i32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    up: bool,
    count: i32,
    members: Vec<usize>,
}

impl<'a> Engraver<'a> {
    pub fn new(score: &'a Score, fonts: &'a FontBook) -> Self {
        Self {
            score,
            fonts,
            draws: DrawList::new(),
            hits: Vec::new(),
            staves: BTreeMap::new(),
            beams: Vec::new(),
            beam_for_note: BTreeMap::new(),
            page_w: page_size_for(score).0,
            page_h: page_size_for(score).1,
        }
    }

    pub fn engrave(mut self) -> EngravedPage {
        self.build_staves();
        self.build_beams();
        self.draw_staff_lines();
        self.draw_barlines_and_braces();
        self.draw_symbols_clefs_keys_meters();
        self.draw_notes_and_rests();
        self.draw_slurs_and_lines();
        self.draw_text_numbers_and_symbols();

        EngravedPage {
            draws: self.draws,
            hits: self.hits,
            staves: self.staves,
            width: self.page_w,
            height: self.page_h,
        }
    }

    fn build_staves(&mut self) {
        for item in self.score.items.iter().filter(|i| i.code == 8) {
            let id = item.staff();
            if !(1..=256).contains(&id) {
                continue;
            }
            let size = {
                let v = item.get(5);
                if v.abs() < 0.001 { 1.0 } else { v.abs() }
            };
            let space = STAFF_SPACE * size;
            let p3 = item.get(3);
            let p6 = item.get(6);
            let x0 = PAGE_LEFT + p3 * SCORE_UNIT;
            let x1 = if p6.abs() < 0.001 {
                PAGE_LEFT + 200.0 * SCORE_UNIT
            } else {
                PAGE_LEFT + p6 * SCORE_UNIT
            };
            let p10 = item.get(10);
            let unit = if self.score.trailer.metric {
                72.0 / 2.54
            } else {
                72.0
            };
            let bottom = if p10.abs() > 0.0001 {
                self.page_h - (PAGE_BOTTOM + p10 * unit)
            } else {
                self.page_h
                    - (PAGE_BOTTOM
                        + (id - 1) as f32 * STAFF_DEFAULT_GAP
                        + item.get(4) * size * STEP)
            };
            let mut lines = item.get(7).round() as i32;
            let mut bottom_level = 3.0;
            if lines == 0 {
                lines = 5;
            }
            if lines >= 100 {
                let hundreds = lines / 100;
                let rest = lines % 100;
                bottom_level = 3.0 + (hundreds.max(1) as f32 - 1.0) * 2.0;
                lines = rest;
            }
            let visible = lines > 0;
            // A one-line percussion staff is legal; P7=0 means the default
            // five lines and P7=-1 means an invisible staff.
            let lines = if !visible { 0 } else { lines.max(1) };
            let top = bottom - (lines - 1) as f32 * space;
            // P11 is the printed line-width increment in 600-dpi pixels.
            let line_width = (0.72 * size + item.get(11) * 0.12).max(0.25);
            self.staves.insert(
                id,
                StaffGeo {
                    id,
                    x0,
                    x1,
                    bottom,
                    top,
                    size,
                    space,
                    lines,
                    visible,
                    bottom_level,
                    line_width,
                },
            );
        }

        // A score may reference a staff that was never explicitly created.
        let used: Vec<i32> = {
            let mut v: Vec<i32> = self
                .score
                .items
                .iter()
                .filter(|i| i.code != 0)
                .map(|i| i.staff())
                .filter(|s| (1..=256).contains(s))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        for id in used {
            self.staves.entry(id).or_insert_with(|| {
                let size = 1.0;
                let space = STAFF_SPACE;
                let bottom = self.page_h - (PAGE_BOTTOM + (id - 1) as f32 * STAFF_DEFAULT_GAP);
                StaffGeo {
                    id,
                    x0: PAGE_LEFT,
                    x1: PAGE_LEFT + 200.0 * SCORE_UNIT,
                    bottom,
                    top: bottom - 4.0 * space,
                    size,
                    space,
                    lines: 5,
                    visible: true,
                    bottom_level: 3.0,
                    line_width: 0.72,
                }
            });
        }
        if self.staves.is_empty() {
            self.staves.insert(
                1,
                StaffGeo {
                    id: 1,
                    x0: PAGE_LEFT,
                    x1: PAGE_LEFT + 200.0 * SCORE_UNIT,
                    bottom: self.page_h - PAGE_BOTTOM,
                    top: self.page_h - PAGE_BOTTOM - 4.0 * STAFF_SPACE,
                    size: 1.0,
                    space: STAFF_SPACE,
                    lines: 5,
                    visible: true,
                    bottom_level: 3.0,
                    line_width: 0.72,
                },
            );
        }
    }

    fn staff(&self, id: i32) -> &StaffGeo {
        self.staves
            .get(&id.max(1))
            .or_else(|| self.staves.values().next())
            .expect("at least one staff")
    }

    fn build_beams(&mut self) {
        for (idx, item) in self.score.items.iter().enumerate() {
            if item.code != 6 {
                continue;
            }
            let staff_id = item.staff();
            if !self.staves.contains_key(&staff_id) {
                continue;
            }
            let p7 = item.get(7).round() as i32;
            let dir = p7.abs() / 10;
            let count = p7.abs() % 10;
            if !(dir == 1 || dir == 2) || count == 0 {
                continue;
            }
            let up = dir == 1;
            let st = self.staff(staff_id).clone();
            let x1 = st.x0 + item.get(3) * SCORE_UNIT;
            let x2 = st.x0 + item.get(6) * SCORE_UNIT;
            if (x2 - x1).abs() < 0.5 && (item.get(3) - item.get(6)).abs() < 0.5 {
                continue;
            }
            let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
            let mut members = Vec::new();
            let mut member_levels = Vec::new();
            for (ni, note) in self.score.items.iter().enumerate() {
                if note.code != 1 || note.staff() != staff_id {
                    continue;
                }
                let nx = st.x0 + note.p3() * SCORE_UNIT;
                if nx < lo - 0.8 || nx > hi + 0.8 {
                    continue;
                }
                let stem = (note.get(5).abs().round() as i32 / 10) % 10;
                if stem == 0 {
                    continue;
                }
                if (stem == 1) != up {
                    continue;
                }
                members.push(ni);
                let mut lv = note.get(4);
                if lv >= 100.0 {
                    lv -= 100.0;
                }
                if lv <= -100.0 {
                    lv += 100.0;
                }
                member_levels.push(lv);
            }
            if members.len() < 2 {
                continue;
            }
            // SCORE stores the intended beam levels in P4/P5, expressed in
            // note-level units.  Prefer those when they are musically sane:
            // this reproduces the original engraver's horizontal or gently
            // sloped beam rather than inventing slope from the outer notes.
            let mut sorted: Vec<(usize, f32, f32, f32)> = members
                .iter()
                .zip(member_levels.iter())
                .map(|(&ni, &lv)| {
                    let nx = st.x0 + self.score.items[ni].p3() * SCORE_UNIT;
                    // P3 is the stem attachment x in SCORE, not the
                    // notehead centre.  The original PDF puts every stem
                    // exactly at x0 + P3 * 2.7.
                    (ni, nx, nx, lv)
                })
                .collect();
            sorted.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            let x1 = sorted
                .first()
                .map(|t| t.2)
                .unwrap_or(st.x0 + item.get(3) * SCORE_UNIT);
            let x2 = sorted
                .last()
                .map(|t| t.2)
                .unwrap_or(st.x0 + item.get(6) * SCORE_UNIT);
            let denom = (x2 - x1).abs();
            let beam_dir = if up { 1.0 } else { -1.0 };

            let mut bl = item.get(4);
            let mut br = item.get(5);
            if bl >= 100.0 {
                bl -= 100.0;
            }
            if bl <= -100.0 {
                bl += 100.0;
            }
            if br >= 100.0 {
                br -= 100.0;
            }
            if br <= -100.0 {
                br += 100.0;
            }
            let mut use_stored = bl.abs() <= 60.0 && br.abs() <= 60.0;
            if use_stored {
                for (_, _, stem_x, lv) in &sorted {
                    let t = if denom < 0.001 {
                        0.0
                    } else {
                        ((stem_x - x1) / (x2 - x1)).clamp(0.0, 1.0)
                    };
                    let line_lv = bl + beam_dir * 7.0 + (br - bl) * t;
                    let stem_steps = if up { line_lv - lv } else { lv - line_lv };
                    if !(3.0..=15.0).contains(&stem_steps) {
                        use_stored = false;
                        break;
                    }
                }
            }
            let (l1, l2) = if use_stored {
                (bl + beam_dir * 7.0, br + beam_dir * 7.0)
            } else {
                let first_level = sorted.first().map(|t| t.3).unwrap_or(7.0);
                let last_level = sorted.last().map(|t| t.3).unwrap_or(first_level);
                let mut l1 = first_level + beam_dir * 7.0;
                let mut l2 = last_level + beam_dir * 7.0;
                for (_, _, stem_x, lv) in &sorted {
                    let t = if denom < 0.001 {
                        0.0
                    } else {
                        ((stem_x - x1) / (x2 - x1)).clamp(0.0, 1.0)
                    };
                    let line_lv = l1 + (l2 - l1) * t;
                    if up && line_lv < lv + 5.0 {
                        let shift = lv + 5.0 - line_lv;
                        l1 += shift;
                        l2 += shift;
                    } else if !up && line_lv > lv - 5.0 {
                        let shift = lv - 5.0 - line_lv;
                        l1 += shift;
                        l2 += shift;
                    }
                }
                (l1, l2)
            };
            let y1 = st.level_y(l1);
            let y2 = st.level_y(l2);
            // A beam object stores the number of complete beams; SCORE
            // engravers also add partial beams for shorter members (an eighth
            // followed by two sixteenths, for example).
            let max_flags = members
                .iter()
                .map(|&ni| note_flag_count(&self.score.items[ni]))
                .max()
                .unwrap_or(1);
            let count = count.max(max_flags).clamp(1, 6);
            // In SCORE, P4/P5 + a seven-step stem give the *outer* edge of the
            // beam stack, i.e. the tip of a fully beamed stem.  The beams grow
            // back toward the noteheads from that line; they do not extend
            // beyond it.  The reference PDF confirms stem tip == outer edge.
            for (ni, _nx, stem_x, _lv) in &sorted {
                let t = if denom < 0.001 {
                    0.0
                } else {
                    ((stem_x - x1) / (x2 - x1)).clamp(0.0, 1.0)
                };
                let tip = y1 + (y2 - y1) * t;
                // The full beam is the outermost one; every stem reaches its
                // outer edge.  Partial beams are added on the notehead side.
                self.beam_for_note.insert(*ni, (self.beams.len(), tip));
            }
            self.beams.push(BeamInfo {
                item: idx,
                staff: staff_id,
                x1,
                x2,
                y1,
                y2,
                up,
                count,
                members,
            });
        }
    }

    fn draw_staff_lines(&mut self) {
        let staves: Vec<StaffGeo> = self.staves.values().cloned().collect();
        for st in staves {
            if !st.visible {
                continue;
            }
            let thickness = st.line_width.max(0.4);
            for i in 0..st.lines {
                let y = st.bottom - i as f32 * st.space;
                self.draws
                    .line(Pt::new(st.x0, y), Pt::new(st.x1, y), Color::INK, thickness);
            }
        }
    }

    fn draw_barlines_and_braces(&mut self) {
        for (idx, item) in self.score.items.iter().enumerate() {
            if item.code != 14 {
                continue;
            }
            let staff_id = item.staff();
            let Some(st) = self.staves.get(&staff_id).cloned() else {
                continue;
            };
            let x = st.x0 + item.get(3) * SCORE_UNIT;
            let p4 = item.get(4).round() as i32;
            let span = if p4 == 0 { 1 } else { p4 };
            let p5 = item.get(5).round() as i32;
            let bottom_staff = st;
            let top_id = if span == 999 {
                self.staves.keys().copied().max().unwrap_or(staff_id)
            } else {
                (staff_id + span - 1).min(256)
            };
            let top_staff = self.staff(top_id).clone();
            let y_bottom = bottom_staff.bottom;
            let y_top = top_staff.top;
            let color = item_color(item);

            match p5 {
                8 => {
                    // Brace.  P7 shifts the brace horizontally.
                    let bx = x - 9.0 + item.get(7) * SCORE_UNIT;
                    self.draw_brace(bx, y_bottom, y_top, &bottom_staff, color);
                }
                9 | 10 => {
                    let bx = x - 5.5;
                    self.draws.line(
                        Pt::new(bx, y_bottom),
                        Pt::new(bx, y_top),
                        color,
                        (1.1 * bottom_staff.size).max(0.6),
                    );
                    let tick = 4.5;
                    self.draws.line(
                        Pt::new(bx, y_bottom),
                        Pt::new(bx + tick, y_bottom),
                        color,
                        1.0,
                    );
                    self.draws
                        .line(Pt::new(bx, y_top), Pt::new(bx + tick, y_top), color, 1.0);
                }
                7 => {
                    let dash = if item.get(8).abs() > 0.01 {
                        item.get(8) * STEP
                    } else {
                        3.0
                    };
                    self.draws.dashed_line(
                        Pt::new(x, y_bottom),
                        Pt::new(x, y_top),
                        color,
                        0.8,
                        dash,
                    );
                }
                _ => self.draw_barline(item, x, y_bottom, y_top, p5, &bottom_staff, color),
            }
            self.add_hit(idx, Pt::new(x - 10.0, y_top), Pt::new(x + 10.0, y_bottom));
        }
    }

    fn draw_brace(&mut self, x: f32, y_bottom: f32, y_top: f32, st: &StaffGeo, color: Color) {
        let h = (y_bottom - y_top).abs();
        let w = (7.0 * st.size).max(4.0);
        let mid = (y_bottom + y_top) * 0.5;
        // SCORE's brace is a broad middle cusp with gently flared ends.
        let mut p = Path::new();
        p.push(PathCmd::MoveTo(Pt::new(x + w, y_top)));
        p.push(PathCmd::CubicTo(
            Pt::new(x - w * 0.1, y_top + h * 0.18),
            Pt::new(x - w * 0.2, mid - h * 0.12),
            Pt::new(x, mid),
        ));
        p.push(PathCmd::CubicTo(
            Pt::new(x - w * 0.2, mid + h * 0.12),
            Pt::new(x - w * 0.1, y_bottom - h * 0.18),
            Pt::new(x + w, y_bottom),
        ));
        self.draws.stroke(p, color, (1.25 * st.size).max(0.8));
    }

    fn draw_barline(
        &mut self,
        item: &Item,
        x: f32,
        y_bottom: f32,
        y_top: f32,
        p5: i32,
        st: &StaffGeo,
        color: Color,
    ) {
        let thin = (0.8 * st.size).max(0.5);
        let thick = (4.5 * st.size).max(2.0);
        let gap = (3.2 * st.size).max(2.0);
        match p5 {
            // P5=1 is the thin line that forms one half of a double bar.
            1 => self
                .draws
                .line(Pt::new(x, y_bottom), Pt::new(x, y_top), color, thin),
            // Double bar: a thin rule followed by a heavier one.
            2 => {
                self.draws.line(
                    Pt::new(x - gap, y_bottom),
                    Pt::new(x - gap, y_top),
                    color,
                    thin,
                );
                self.draws
                    .rect(x - 0.5, y_top, thick, y_bottom - y_top, color);
            }
            // Final bar: the heavy rule carries the finality; no repeat dots.
            3 => {
                self.draws.line(
                    Pt::new(x - gap, y_bottom),
                    Pt::new(x - gap, y_top),
                    color,
                    thin,
                );
                self.draws
                    .rect(x + thick * 0.55, y_top, thick, y_bottom - y_top, color);
            }
            // End repeat: dots on the left of thin|thick.
            4 => {
                self.draw_repeat_dots(item, x - gap * 2.2, st, color);
                self.draws.line(
                    Pt::new(x - gap, y_bottom),
                    Pt::new(x - gap, y_top),
                    color,
                    thin,
                );
                self.draws
                    .rect(x - 0.5, y_top, thick, y_bottom - y_top, color);
            }
            // Start repeat: thick|thin with the dots on the right.
            5 => {
                self.draws
                    .rect(x - gap - thick, y_top, thick, y_bottom - y_top, color);
                self.draws
                    .line(Pt::new(x, y_bottom), Pt::new(x, y_top), color, thin);
                self.draw_repeat_dots(item, x + gap * 1.5, st, color);
            }
            // Double repeat: thick|thin on the left, thin|thick on the right,
            // with the two pairs of dots between the rules.
            6 => {
                self.draws
                    .rect(x - gap * 2.0 - thick, y_top, thick, y_bottom - y_top, color);
                self.draws.line(
                    Pt::new(x - gap * 1.15, y_bottom),
                    Pt::new(x - gap * 1.15, y_top),
                    color,
                    thin,
                );
                self.draw_repeat_dots(item, x - gap * 0.45, st, color);
                self.draw_repeat_dots(item, x + gap * 0.45, st, color);
                self.draws.line(
                    Pt::new(x + gap * 1.15, y_bottom),
                    Pt::new(x + gap * 1.15, y_top),
                    color,
                    thin,
                );
                self.draws
                    .rect(x + gap * 2.0, y_top, thick, y_bottom - y_top, color);
            }
            _ => {
                self.draws
                    .line(Pt::new(x, y_bottom), Pt::new(x, y_top), color, thin);
            }
        }
    }

    fn draw_repeat_dots(&mut self, item: &Item, x: f32, st: &StaffGeo, color: Color) {
        let mut p16 = item.get(16).round() as i32;
        let (l1, l2) = if p16 == 0 {
            (6.0, 8.0)
        } else {
            if p16 < 0 {
                p16 = -p16;
            }
            ((p16 / 100) as f32, (p16 % 100) as f32)
        };
        let r = (0.72 * st.size).max(0.5);
        for level in [l1, l2] {
            let y = st.level_y(level);
            self.draws.fill(circle_path(x, y, r), color);
        }
    }

    fn draw_symbols_clefs_keys_meters(&mut self) {
        for (idx, item) in self.score.items.iter().enumerate() {
            match item.code {
                3 => self.draw_clef(idx, item),
                17 => self.draw_key_signature(idx, item),
                18 => self.draw_meter(idx, item),
                9 => {
                    let p5 = item.get(5).round() as i32;
                    if (0..=5).contains(&p5) && p5 != 5 {
                        self.draw_clef_symbol(idx, item, p5);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_clef(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let p5 = item.get(5);
        let name = clef_glyph(p5);
        let Some(name) = name else { return };
        let mut p4 = item.get(4);
        let mut size = if item.get(6).abs() < 0.001 {
            1.0
        } else {
            item.get(6)
        };
        if p4 >= 100.0 {
            p4 -= 100.0;
            size *= 0.58;
        }
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(15) * 0.35;
        let y = st.level_y(7.0 + p4);
        let font_size = 4.0 * st.space * size;
        if let Some(path) = self.fonts.music_glyph_centered(name, font_size, x, y, 0.0) {
            self.draws.fill(path, item_color(item));
        }
        self.add_hit(
            idx,
            Pt::new(x - st.space * 1.5, y - st.space * 3.0),
            Pt::new(x + st.space * 1.5, y + st.space * 3.0),
        );
    }

    fn draw_clef_symbol(&mut self, idx: usize, item: &Item, p5: i32) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let glyph = match p5 {
            0 => G_TREBLE,
            1 => G_BASS,
            2 => G_ALTO,
            3 => G_TENOR,
            4 => G_PERCUSSION,
            _ => return,
        };
        let x = st.x0 + item.get(3) * SCORE_UNIT;
        let y = st.level_y(7.0 + item.get(4));
        let size = if item.get(6).abs() < 0.001 {
            1.0
        } else {
            item.get(6)
        };
        if let Some(path) =
            self.fonts
                .music_glyph_centered(glyph, 4.0 * st.space * size, x, y, item.get(9))
        {
            self.draws.fill(path, item_color(item));
        }
        self.add_hit(
            idx,
            Pt::new(x - 10.0, y - 25.0),
            Pt::new(x + 10.0, y + 25.0),
        );
    }

    fn draw_key_signature(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let mut count = item.get(5).round() as i32;
        let natural = count.abs() >= 100;
        let cue = item.get(6).abs() >= 100.0;
        if count >= 100 {
            count -= 100;
        } else if count <= -100 {
            count += 100;
        }
        let clef = (item.get(6).abs().round() as i32) % 100;
        let spacing = if item.get(7).abs() < 0.001 {
            1.0
        } else {
            item.get(7)
        };
        let size_factor = if cue { 0.6 } else { 1.0 };
        let x0 = st.x0 + item.get(3) * SCORE_UNIT;
        let yshift = item.get(4);
        let glyph = if natural {
            G_NATURAL
        } else if count > 0 {
            G_SHARP
        } else {
            G_FLAT
        };
        let levels = key_signature_levels(clef, count);
        let glyph_size = 4.0 * st.space * size_factor;
        for (i, level) in levels.iter().enumerate() {
            let x = x0 + i as f32 * st.space * 1.05 * spacing;
            let y = st.level_y(*level + yshift);
            if let Some(path) = self
                .fonts
                .music_glyph_centered(glyph, glyph_size, x, y, 0.0)
            {
                self.draws.fill(path, item_color(item));
            }
        }
        let width = (levels.len().max(1) as f32) * st.space;
        self.add_hit(
            idx,
            Pt::new(x0 - 4.0, st.level_y(12.0)),
            Pt::new(x0 + width, st.level_y(2.0)),
        );
    }

    fn draw_meter(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(15) * 0.35;
        let yoff = item.get(4);
        let p5 = item.get(5).round() as i32;
        let p6 = item.get(6).round() as i32;
        let mut scale = item.get(7);
        if scale.abs() < 0.001 {
            scale = 1.0;
        }
        let color = item_color(item);
        if p5 >= 99 {
            let glyph = if p6 >= 2 { G_TIME_CUT } else { G_TIME_COMMON };
            let size = 4.6 * st.space * scale;
            if let Some(path) =
                self.fonts
                    .music_glyph_centered(glyph, size, x, st.level_y(7.0 + yoff), 0.0)
            {
                self.draws.fill(path, color);
            }
            self.add_hit(
                idx,
                Pt::new(x - 14.0, st.level_y(10.0)),
                Pt::new(x + 14.0, st.level_y(4.0)),
            );
            return;
        }

        let size = 4.0 * st.space * scale;
        let top_text = p5.to_string();
        let bottom_text = p6.max(1).to_string();
        let center_y = st.level_y(7.0 + yoff);
        let top_center = center_y - st.space * 1.05 * scale;
        let bottom_center = center_y + st.space * 1.05 * scale;
        self.draw_meter_digits(&top_text, x, top_center, size, color);
        self.draw_meter_digits(&bottom_text, x, bottom_center, size, color);
        self.add_hit(
            idx,
            Pt::new(x - 14.0, top_center - size * 0.5),
            Pt::new(x + 14.0, bottom_center + size * 0.5),
        );
    }

    fn draw_meter_digits(&mut self, text: &str, cx: f32, cy: f32, size: f32, color: Color) {
        let chars: Vec<char> = text.chars().collect();
        let spacing = size * 0.52;
        let total = spacing * (chars.len().saturating_sub(1)) as f32;
        for (i, ch) in chars.into_iter().enumerate() {
            if let Some(d) = ch.to_digit(10) {
                let glyph = char::from_u32(0xE080 + d).unwrap_or('?');
                let x = cx - total * 0.5 + i as f32 * spacing;
                if let Some(path) = self.fonts.music_glyph_centered(glyph, size, x, cy, 0.0) {
                    self.draws.fill(path, color);
                }
            }
        }
    }

    fn draw_notes_and_rests(&mut self) {
        let mut notes: Vec<(usize, NoteInfo)> = self
            .score
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.code == 1)
            .filter_map(|(idx, item)| NoteInfo::new(idx, item, &self.staves).map(|n| (idx, n)))
            .collect();

        // Stemless notes in a chord share the stem direction of the chord's
        // stemmed member.  Otherwise their noteheads drift to the wrong side
        // of the stem (the "same quarter note on both left and right" bug).
        let chord_up: Vec<Option<bool>> = notes
            .iter()
            .map(|(_, n)| {
                if n.stemmed || matches!(n.head_code, 2 | 3 | 7) {
                    return None;
                }
                notes
                    .iter()
                    .find(|(_, m)| {
                        m.stemmed
                            && m.staff == n.staff
                            && (m.x - n.x).abs() < 0.01
                            && (m.level - n.level).abs() > 0.1
                    })
                    .map(|(_, m)| m.up)
            })
            .collect();
        for (n, up) in notes.iter_mut().zip(chord_up) {
            if let Some(up) = up {
                n.1.up = up;
            }
        }

        // Stems and beams are drawn beneath the noteheads.
        for (idx, n) in &notes {
            self.draw_note_stem(*idx, n);
        }
        let beams = self.beams.clone();
        for b in &beams {
            self.draw_beam(b);
        }

        for (idx, n) in &notes {
            self.draw_notehead(*idx, n);
        }

        for (idx, item) in self.score.items.iter().enumerate() {
            if item.code == 2 {
                self.draw_rest(idx, item);
            }
        }
    }

    fn draw_note_stem(&mut self, idx: usize, n: &NoteInfo) {
        if n.headless || !n.stemmed {
            return;
        }
        let Some(st) = self.staves.get(&n.staff).cloned() else {
            return;
        };
        let stem_x = n.x;
        let default_end = if n.up {
            n.y - (3.5 + n.stem_extra) * st.space
        } else {
            n.y + (3.5 + n.stem_extra) * st.space
        };
        let end_y = if let Some((_, by)) = self.beam_for_note.get(&idx) {
            *by
        } else {
            default_end
        };
        self.draws.line(
            Pt::new(stem_x, n.y),
            Pt::new(stem_x, end_y),
            n.color,
            (0.62 * st.size).max(0.45),
        );

        if !self.beam_for_note.contains_key(&idx) && n.flags > 0 {
            self.draw_note_flags(n, stem_x, end_y, &st);
        }
    }

    fn draw_note_flags(&mut self, n: &NoteInfo, stem_x: f32, end_y: f32, st: &StaffGeo) {
        let count = n.flags.min(6);
        let base = if n.up { 0xE240 } else { 0xE241 };
        for i in 0..count {
            let cp = base + (i as u32) * 2;
            let c = char::from_u32(cp).unwrap_or('?');
            let _ = i;
            // SMuFL flag glyphs have their origin at the stem attachment
            // point; centring them pushed the hook past the stem tip.
            if let Some(path) =
                self.fonts
                    .music_glyph_baseline(c, 3.4 * st.space * n.size, stem_x, end_y, 0.0)
            {
                self.draws.fill(path, n.color);
            }
        }
    }

    fn draw_beam(&mut self, b: &BeamInfo) {
        let Some(st) = self.staves.get(&b.staff).cloned() else {
            return;
        };
        let color = item_color(&self.score.items[b.item]);
        // Measured from the original WinScore output: a beam is exactly half
        // a staff space thick and successive beams are 3/4 of a space apart.
        // P4/P5 + 7 steps define the outer edge (stem tip); the stack grows
        // back toward the noteheads.
        let thickness = (0.50 * st.space).max(1.8);
        let gap = (0.75 * st.space).max(2.8);
        // Page y grows downwards.  "away" points from the notes towards the
        // beam tip: negative for up-stems, positive for down-stems.
        let away = if b.up { -1.0 } else { 1.0 };
        // The full-group beam is outermost; extra beams grow toward the notes.
        let layer_offset = |level: f32| -away * (level * gap + thickness * 0.5);
        let mut members: Vec<(usize, f32, i32)> = b
            .members
            .iter()
            .map(|&ni| {
                let it = &self.score.items[ni];
                let nx = st.x0 + it.p3() * SCORE_UNIT;
                (ni, nx, note_flag_count(it))
            })
            .collect();
        members.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // The first beam runs the full width of the group and its outer edge
        // is exactly the stem tip.
        let primary_off = layer_offset(0.0);
        self.draws.line(
            Pt::new(b.x1, b.y1 + primary_off),
            Pt::new(b.x2, b.y2 + primary_off),
            color,
            thickness,
        );

        // Secondary beams follow the rhythm: a break occurs wherever a note
        // needs fewer beams.  Isolated short notes receive a partial stub.
        let beam_at = |x: f32| {
            let t = if (b.x2 - b.x1).abs() < 0.001 {
                0.0
            } else {
                ((x - b.x1) / (b.x2 - b.x1)).clamp(0.0, 1.0)
            };
            b.y1 + (b.y2 - b.y1) * t
        };
        for level in 1..b.count.max(1) {
            let required = level + 1;
            let offset = layer_offset(level as f32);
            let mut run: Vec<usize> = Vec::new();
            let flush = |engraver: &mut Engraver, run: &[usize]| {
                if run.is_empty() {
                    return;
                }
                if run.len() >= 2 {
                    // One continuous bar per rhythmic run.  Drawing each pair
                    // separately leaves visible seams in a thick beam.
                    let xa = members[run[0]].1;
                    let xb = members[*run.last().unwrap()].1;
                    engraver.draws.line(
                        Pt::new(xa, beam_at(xa) + offset),
                        Pt::new(xb, beam_at(xb) + offset),
                        color,
                        thickness,
                    );
                } else {
                    let idx = run[0];
                    let x = members[idx].1;
                    let toward_right = idx + 1 < members.len();
                    let dx = if toward_right { 0.75 } else { -0.75 } * st.space;
                    engraver.draws.line(
                        Pt::new(x, beam_at(x) + offset),
                        Pt::new(x + dx, beam_at(x + dx) + offset),
                        color,
                        thickness,
                    );
                }
            };
            for (i, &(_ni, _x, flags)) in members.iter().enumerate() {
                if flags >= required {
                    run.push(i);
                } else {
                    flush(self, &run);
                    run.clear();
                }
            }
            flush(self, &run);
        }
        if b.count > 0 {
            let p8 = self.score.items[b.item].get(8);
            if p8 > 0.5 {
                let n = (p8 as i32) % 1000;
                if n > 0 {
                    let mx = (b.x1 + b.x2) * 0.5;
                    let my = (b.y1 + b.y2) * 0.5 - if b.up { 10.0 } else { -18.0 };
                    self.draw_plain_text(
                        &format!("{n}"),
                        mx - 3.0,
                        my,
                        10.0 * st.size * 0.95,
                        2,
                        color,
                        0.0,
                    );
                }
            }
        }
        self.add_hit(
            b.item,
            Pt::new(b.x1.min(b.x2), b.y1.min(b.y2) - 8.0),
            Pt::new(b.x1.max(b.x2), b.y1.max(b.y2) + 8.0),
        );
    }

    fn draw_notehead(&mut self, idx: usize, n: &NoteInfo) {
        let Some(st) = self.staves.get(&n.staff).cloned() else {
            return;
        };
        let glyph = n.head_glyph();
        let head_size = 4.0 * st.space * n.size;
        let hx = n.head_x(st.space);
        if !n.headless && n.head_code >= 0 {
            if let Some(g) = glyph {
                if let Some(path) = self.fonts.music_glyph_centered(g, head_size, hx, n.y, 0.0) {
                    self.draws.fill(path, n.color);
                }
            }
        }
        if n.accidental != 0 {
            if let Some(g) = accidental_glyph(n.accidental) {
                let ax = hx - 0.92 * st.space - n.accidental_space * st.space;
                if let Some(path) =
                    self.fonts
                        .music_glyph_centered(g, 4.0 * st.space * n.size, ax, n.y, 0.0)
                {
                    self.draws.fill(path, n.color);
                }
                if n.acc_parens {
                    let paren = "(";
                    let shaped = self
                        .fonts
                        .shape_plain(paren, 4.0 * st.space * n.size, 0, 1.0);
                    for p in self.fonts.text_run_paths(
                        &shaped,
                        4.0 * st.space * n.size,
                        Pt::new(ax - 1.2 * st.space, n.y),
                        0.0,
                    ) {
                        self.draws.fill(p, n.color);
                    }
                }
            }
        }
        self.draw_ledger_lines(n, &st);
        // Dots.
        if n.dots > 0 {
            // Measured from the WinScore PDF: the augmentation dot is only
            // about 0.23 of a staff space across (radius ≈ 0.115 space).
            let r = (0.115 * st.space).max(0.55);
            // On a line the dot goes in the adjacent space (half a space
            // away); in a space it goes in the next space (a full space).
            let dot_y = if n.level.round() as i32 % 2 == 0 {
                n.y - st.space
            } else {
                n.y - st.space * 0.5
            };
            for i in 0..n.dots {
                let x = hx + (1.0 + i as f32 * 0.55) * st.space;
                self.draws.fill(circle_path(x, dot_y, r * n.size), n.color);
            }
        }
        // Articulations.
        if n.mark != 0.0 {
            self.draw_note_mark(n, &st);
        }
        self.add_hit(
            idx,
            Pt::new(hx - 1.3 * st.space, n.y - 1.0 * st.space),
            Pt::new(hx + 1.3 * st.space, n.y + 1.0 * st.space),
        );
    }

    fn draw_ledger_lines(&mut self, n: &NoteInfo, st: &StaffGeo) {
        if n.no_ledger {
            return;
        }
        let level = n.level;
        let hx = n.head_x(st.space);
        let half_w = 0.92 * st.space * n.size;
        let thickness = (0.7 * st.size).max(0.45);
        if level > 11.0 {
            let mut l = 13.0;
            while l <= level + 0.001 {
                let y = st.level_y(l);
                self.draws.line(
                    Pt::new(hx - half_w, y),
                    Pt::new(hx + half_w, y),
                    n.color,
                    thickness,
                );
                l += 2.0;
            }
        } else if level < 3.0 {
            let mut l = 1.0;
            while l >= level - 0.001 {
                let y = st.level_y(l);
                self.draws.line(
                    Pt::new(hx - half_w, y),
                    Pt::new(hx + half_w, y),
                    n.color,
                    thickness,
                );
                l -= 2.0;
            }
        }
    }

    fn draw_note_mark(&mut self, n: &NoteInfo, st: &StaffGeo) {
        let mut mark = n.mark.abs().round() as i32 % 100;
        if mark >= 32 {
            return;
        }
        let mut items: Vec<char> = Vec::new();
        match mark {
            4 => items.push(G_MARCATO),
            5 => items.push(G_ACCENT),
            6 => items.push(G_TENUTO),
            7 => items.push(G_STACCATO),
            8 => {
                items.push(G_MARCATO);
                items.push(G_STACCATO);
            }
            9 => {
                items.push(G_ACCENT);
                items.push(G_STACCATO);
            }
            10 => {
                items.push(G_TENUTO);
                items.push(G_STACCATO);
            }
            11 => items.push(G_DOWNBOW),
            12 => items.push(G_UPBOW),
            14 => items.push(G_FERMATA),
            17 => items.push(G_MORDENT),
            18 => items.push(G_INVERTED_MORDENT),
            19 => items.push(G_MARCATO),
            24 => items.push(G_PLUS),
            25 => {
                items.push(G_ACCENT);
                items.push(G_TENUTO);
            }
            _ => {}
        }
        let on_stem_side = n.mark < 0.0;
        let below = if n.up { !on_stem_side } else { on_stem_side };
        let dir = if below { 1.0 } else { -1.0 };
        let size = 4.0 * st.space * n.size * 0.8;
        for (i, c) in items.into_iter().enumerate() {
            let y = n.y + dir * (1.15 + i as f32 * 0.8) * st.space;
            if let Some(path) = self
                .fonts
                .music_glyph_centered(c, size, n.head_x(st.space), y, 0.0)
            {
                self.draws.fill(path, n.color);
            }
        }
        let _ = mark;
        mark = 0;
        let _ = mark;
    }

    fn draw_rest(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let p5 = item.get(5).round() as i32;
        let p6 = item.get(6).round() as i32;
        let p4 = item.get(4);
        let mut level = p4;
        let mut size = item.get(15);
        if size.abs() < 0.001 {
            size = 1.0;
        }
        if level >= 100.0 {
            level -= 100.0;
            size *= 0.58;
        }
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(10) * 0.055 * st.space;
        let color = item_color(item);
        if p6 == -1 {
            // Invisible rests are still selectable.
            self.add_hit(
                idx,
                Pt::new(x - 8.0, st.level_y(level + 2.0)),
                Pt::new(x + 8.0, st.level_y(level - 2.0)),
            );
            return;
        }
        if p5 <= -2 {
            // Multi-measure / whole-measure rest, optionally with a count.
            let line_y = st.level_y(9.0);
            self.draws.rect(
                x - 0.8 * st.space,
                line_y,
                1.6 * st.space,
                0.22 * st.space,
                color,
            );
            if item.get(8) > 1.0 {
                let label = format!("{}", item.get(8).round() as i32);
                self.draw_plain_text(
                    &label,
                    x,
                    line_y - 1.5 * st.space,
                    10.0 * st.size,
                    1,
                    color,
                    0.0,
                );
            }
            self.add_hit(
                idx,
                Pt::new(x - st.space, line_y - 1.4 * st.space),
                Pt::new(x + st.space, line_y + 0.5 * st.space),
            );
            return;
        }
        let glyph = match p5 {
            -1 => G_REST_HALF,
            0 => G_REST_QUARTER,
            1 => G_REST_8TH,
            2 => G_REST_16TH,
            3 => G_REST_32ND,
            4 => G_REST_64TH,
            _ => G_REST_QUARTER,
        };
        let mut cy = st.level_y(level);
        if p5 == -1 {
            cy = st.level_y(7.0);
        } else if p5 >= 0 {
            cy = st.level_y(7.0);
        }
        if let Some(path) =
            self.fonts
                .music_glyph_centered(glyph, 4.0 * st.space * size, x, cy, 0.0)
        {
            self.draws.fill(path, color);
        }
        if item.get(8) == -1.0 || item.get(8) == -2.0 {
            let c = G_FERMATA;
            let fy = if item.get(8) == -1.0 {
                st.level_y(11.5)
            } else {
                st.level_y(2.0)
            };
            if let Some(path) =
                self.fonts
                    .music_glyph_centered(c, 4.0 * st.space * size, x, fy, 0.0)
            {
                self.draws.fill(path, color);
            }
        }
        if p6 > 0 {
            let r = (0.37 * st.space).max(0.8);
            for i in 0..p6 {
                let dx = x + (1.0 + i as f32 * 0.55) * st.space;
                self.draws.fill(circle_path(dx, st.level_y(7.0), r), color);
            }
        }
        self.add_hit(
            idx,
            Pt::new(x - 0.8 * st.space, cy - 1.8 * st.space),
            Pt::new(x + 0.8 * st.space, cy + 1.8 * st.space),
        );
    }

    fn draw_slurs_and_lines(&mut self) {
        for (idx, item) in self.score.items.iter().enumerate() {
            match item.code {
                5 => self.draw_slur(idx, item),
                4 => self.draw_line_item(idx, item),
                7 => self.draw_trill_item(idx, item),
                _ => {}
            }
        }
    }

    fn draw_slur(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let x1 = st.x0 + item.get(3) * SCORE_UNIT;
        let x2 = st.x0 + item.get(6) * SCORE_UNIT;
        let y1 = st.level_y(item.get(4));
        let y2 = st.level_y(item.get(5));
        let p8 = item.get(8).round() as i32;
        let color = item_color(item);
        if p8 == 1 || p8 == 2 || p8 == 4 {
            // First/second ending bracket.
            let num = if p8 == 4 { item.get(9) as i32 } else { p8 };
            let top = y1.min(y2) - 1.2 * st.space;
            self.draws
                .line(Pt::new(x1, y1), Pt::new(x1, top), color, 1.1);
            self.draws
                .line(Pt::new(x1, top), Pt::new(x2, top), color, 1.1);
            self.draws
                .line(Pt::new(x2, top), Pt::new(x2, y2), color, 1.1);
            self.draw_plain_text(
                &format!("{num}."),
                x1 + 0.5 * st.space,
                top - 2.0,
                10.0 * st.size,
                2,
                color,
                0.0,
            );
            self.add_hit(idx, Pt::new(x1, top - 12.0), Pt::new(x2, y1.max(y2)));
            return;
        }
        if p8 == 3 {
            let num = if item.get(9).abs() > 0.5 {
                item.get(9) as i32
            } else {
                3
            };
            let top = y1.min(y2) - 1.4 * st.space;
            self.draws
                .line(Pt::new(x1, y1), Pt::new(x1, top), color, 1.0);
            self.draws
                .line(Pt::new(x1, top), Pt::new(x2, top), color, 1.0);
            self.draws
                .line(Pt::new(x2, top), Pt::new(x2, y2), color, 1.0);
            self.draw_plain_text(
                &format!("{num}"),
                (x1 + x2) * 0.5,
                top - 2.0,
                10.0 * st.size,
                2,
                color,
                0.0,
            );
            self.add_hit(idx, Pt::new(x1, top - 12.0), Pt::new(x2, y1.max(y2)));
            return;
        }

        let mut curve = item.get(7);
        if curve.abs() < 0.001 {
            curve = -2.0;
        }
        let dir = if curve < 0.0 { -1.0 } else { 1.0 };
        let amount = curve.abs() * st.size * STEP;
        let midy = (y1 + y2) * 0.5 + dir * -amount;
        let c1 = Pt::new(x1 + (x2 - x1) * 0.25, midy);
        let c2 = Pt::new(x1 + (x2 - x1) * 0.75, midy);
        let mut p = Path::new();
        p.push(PathCmd::MoveTo(Pt::new(x1, y1)));
        p.push(PathCmd::CubicTo(c1, c2, Pt::new(x2, y2)));
        let thickness = if item.get(13).abs() > 0.001 {
            (item.get(13) * 0.8).max(0.5)
        } else {
            (1.1 * st.size).max(0.7)
        };
        if item.get(11).abs() > 0.5 {
            self.draws
                .dotted_line(Pt::new(x1, y1), Pt::new(x2, y2), color, thickness, 4.0, 3.0);
        } else {
            self.draws.stroke(p, color, thickness);
        }
        if item.get(16).abs() > 0.5 {
            let n = item.get(16) as i32;
            self.draw_plain_text(
                &format!("{n}"),
                (x1 + x2) * 0.5,
                midy - 0.8 * st.space,
                10.0 * st.size,
                2,
                color,
                0.0,
            );
        }
        self.add_hit(
            idx,
            Pt::new(x1.min(x2), midy.min(y1).min(y2) - 4.0),
            Pt::new(x1.max(x2), midy.max(y1).max(y2) + 4.0),
        );
    }

    fn draw_line_item(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let p3 = item.get(3);
        let p6 = item.get(6);
        let x1 = st.x0 + p3 * SCORE_UNIT;
        let x2 = if p6.abs() < 0.001 {
            x1
        } else {
            st.x0 + p6 * SCORE_UNIT
        };
        let y1 = st.level_y(item.get(4));
        let p5 = item.get(5);
        let color = item_color(item);
        if p5.round() == 999.0 {
            // Crescendo / decrescendo hairpin.
            let spread = if item.get(7).abs() > 0.01 {
                item.get(7).abs() * st.size * STEP
            } else {
                2.3 * st.size * STEP
            };
            let mid = y1;
            let mut a = Path::new();
            a.push(PathCmd::MoveTo(Pt::new(x1, mid - spread * 0.5)));
            a.push(PathCmd::LineTo(Pt::new(x2, mid)));
            a.push(PathCmd::LineTo(Pt::new(x1, mid + spread * 0.5)));
            self.draws.stroke(a, color, 0.9);
            self.add_hit(idx, Pt::new(x1, mid - spread), Pt::new(x2, mid + spread));
            return;
        }
        let y2 = st.level_y(p5);
        let p7 = item.get(7).round() as i32;
        if p7 == -5 || (x1 - x2).abs() < 1.0 {
            self.draws
                .line(Pt::new(x1, y1), Pt::new(x1, y2), color, 0.9);
            self.add_hit(
                idx,
                Pt::new(x1 - 5.0, y1.min(y2)),
                Pt::new(x1 + 5.0, y1.max(y2)),
            );
            return;
        }
        match p7 {
            -4 => self
                .draws
                .dotted_line(Pt::new(x1, y1), Pt::new(x2, y2), color, 0.8, 2.0, 3.0),
            -1 | -2 | -3 => {
                self.draw_wave(x1, y1, x2, y2, color, p7.abs() as f32);
            }
            _ => self
                .draws
                .line(Pt::new(x1, y1), Pt::new(x2, y2), color, 0.8),
        }
        self.add_hit(
            idx,
            Pt::new(x1.min(x2), y1.min(y2) - 4.0),
            Pt::new(x1.max(x2), y1.max(y2) + 4.0),
        );
    }

    fn draw_wave(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Color, amp: f32) {
        let len = ((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)).sqrt();
        if len < 1.0 {
            return;
        }
        let n = (len / 5.0).max(2.0) as usize;
        let mut p = Path::new();
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let x = x1 + (x2 - x1) * t;
            let y = y1 + (y2 - y1) * t + amp.sin() * (if i % 2 == 0 { 1.0 } else { -1.0 });
            if i == 0 {
                p.push(PathCmd::MoveTo(Pt::new(x, y)));
            } else {
                p.push(PathCmd::LineTo(Pt::new(x, y)));
            }
        }
        self.draws.stroke(p, color, 0.8);
    }

    fn draw_trill_item(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let x = st.x0 + item.get(3) * SCORE_UNIT;
        let y = st.level_y(item.get(4));
        let p7 = item.get(7).round() as i32;
        let color = item_color(item);
        let x2 = if item.get(6).abs() > 0.001 {
            st.x0 + item.get(6) * SCORE_UNIT
        } else {
            x
        };
        match p7 {
            // P7 0/1 = trill, 2 = mordent, 3 = inverted mordent, 4 = turn.
            0 | 1 => {
                self.draw_plain_text("tr", x, y, 10.0 * st.size, 2, color, 0.0);
                if x2 > x {
                    self.draws.dotted_line(
                        Pt::new(x + 10.0, y - 2.0),
                        Pt::new(x2, y - 2.0),
                        color,
                        0.8,
                        2.0,
                        2.0,
                    );
                }
            }
            2 | 3 => {
                let glyph = if p7 == 2 {
                    G_MORDENT
                } else {
                    G_INVERTED_MORDENT
                };
                if let Some(path) = self
                    .fonts
                    .music_glyph_centered(glyph, 4.0 * st.size, x, y, 0.0)
                {
                    self.draws.fill(path, color);
                }
            }
            4 => {
                if let Some(path) =
                    self.fonts
                        .music_glyph_centered(G_TURN, 4.0 * st.size, x, y, 0.0)
                {
                    self.draws.fill(path, color);
                }
            }
            // P7 5/6 are 8va/15ma above, 7/8 are 8vb/15mb below.  The values
            // 15/155/-8/-88 are kept for files written by older builds of
            // this reimplementation.
            5 | 6 | 7 | 8 | -8 | -88 | 15 | 155 => {
                let (label, above) = match p7 {
                    7 | 8 | -8 | -88 => {
                        if p7 == -8 || p7 == -88 {
                            ("15mb", false)
                        } else {
                            ("8vb", false)
                        }
                    }
                    6 | 15 | 155 => ("15ma", true),
                    _ => ("8va", true),
                };
                let ty = if above { y - 8.0 } else { y + 12.0 };
                self.draw_plain_text(label, x, ty, 9.0 * st.size, 2, color, 0.0);
                if x2 > x {
                    self.draws.dotted_line(
                        Pt::new(x + 25.0, ty - 2.0),
                        Pt::new(x2, ty - 2.0),
                        color,
                        0.8,
                        4.0,
                        2.0,
                    );
                }
            }
            20 => {
                self.draw_plain_text("Ped.", x, y, 10.0 * st.size, 2, color, 0.0);
            }
            21 | 22 => {
                self.draw_plain_text(
                    if p7 == 21 { "Ped." } else { "*" },
                    x,
                    y,
                    10.0 * st.size,
                    2,
                    color,
                    0.0,
                );
                if x2 > x {
                    self.draws
                        .line(Pt::new(x + 18.0, y + 2.0), Pt::new(x2, y + 2.0), color, 0.8);
                }
            }
            _ => {}
        }
        self.add_hit(
            idx,
            Pt::new(x, y - 12.0),
            Pt::new(x2.max(x + 20.0), y + 12.0),
        );
    }

    fn draw_text_numbers_and_symbols(&mut self) {
        for (idx, item) in self.score.items.iter().enumerate() {
            match item.code {
                16 => self.draw_text(idx, item),
                10 => self.draw_number(idx, item),
                9 => self.draw_symbol(idx, item),
                12 => self.draw_box(idx, item),
                _ => {}
            }
        }
    }

    fn draw_text(&mut self, idx: usize, item: &Item) {
        let Some(text) = &item.text else { return };
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let size = text_font_size(item, st.size);
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(11) * 0.35;
        let y = st.level_y(item.get(4));
        let tracking = if item.get(5).abs() < 0.001 {
            1.0
        } else {
            item.get(5)
        };
        let space_factor = if item.get(10).abs() < 0.001 {
            1.0
        } else {
            item.get(10)
        };
        let font_override = {
            let p8 = item.get(8);
            if p8 >= 1000.0 { Some(p8 as i32) } else { None }
        };
        let mut shaped = self.fonts.shape(
            text.visible_bytes(),
            size,
            tracking,
            space_factor,
            font_override,
        );
        if shaped.glyphs.is_empty() {
            return;
        }
        let p13 = item.get(13);
        if p13 > 0.5 {
            let target = p13 * SCORE_UNIT;
            if target > 1.0 && shaped.width > 1.0 {
                let factor = target / shaped.width;
                if (0.72..=1.32).contains(&factor) {
                    for g in &mut shaped.glyphs {
                        g.x *= factor;
                        g.advance *= factor;
                    }
                    shaped.width = target;
                }
            }
        }
        let rotate = item.get(9);
        let color = item_color(item);
        for p in self
            .fonts
            .text_run_paths(&shaped, size, Pt::new(x, y), rotate)
        {
            self.draws.fill(p, color);
        }
        let height = size * 1.2;
        self.add_hit(
            idx,
            Pt::new(x, y - height),
            Pt::new(x + shaped.width.max(6.0), y + size * 0.35),
        );
    }

    fn draw_number(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let label = number_label(item);
        if label.is_empty() {
            return;
        }
        let mut size = item.get(6);
        if size.abs() < 0.001 {
            size = 1.0;
        }
        size = size.abs() % 100.0;
        if size < 0.01 {
            size = 1.0;
        }
        let size_pt = 10.0 * st.size * size;
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(15) * 0.35;
        let y = st.level_y(item.get(4));
        let color = item_color(item);
        let shaped = self
            .fonts
            .shape_plain(&label, size_pt, item.get(7) as i32, 1.0);
        let w = shaped.width;
        let h = size_pt;
        let p8 = item.get(8).round() as i32;
        if p8 == 1 || p8 == 2 {
            let pad = 3.0;
            if p8 == 1 {
                let cy = y - h * 0.32;
                let r = (w * 0.5 + pad).max(h * 0.6);
                self.draws
                    .stroke(circle_path(x + w * 0.5, cy, r), color, 0.8);
            } else {
                let bx = x - pad;
                let by = y - h * 0.95;
                self.draws
                    .stroke_rect(bx, by, w + 2.0 * pad, h * 1.25, color, 0.8);
            }
        }
        if item.get(19).abs() > 0.01 {
            for p in self
                .fonts
                .text_run_paths(&shaped, size_pt, Pt::new(x, y), item.get(19))
            {
                self.draws.fill(p, color);
            }
        } else {
            for p in self
                .fonts
                .text_run_paths(&shaped, size_pt, Pt::new(x, y), 0.0)
            {
                self.draws.fill(p, color);
            }
        }
        self.add_hit(idx, Pt::new(x, y - h), Pt::new(x + w.max(5.0), y + h * 0.4));
    }

    fn draw_symbol(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let p5 = item.get(5).round() as i32;
        let mut size = item.get(6);
        if size.abs() < 0.001 {
            size = 1.0;
        }
        size = size.abs();
        if size > 100.0 {
            size -= 100.0;
        }
        if size > 100.0 {
            size -= 100.0;
        }
        if size.abs() < 0.01 {
            size = 1.0;
        }
        let x = st.x0 + item.get(3) * SCORE_UNIT + item.get(15) * 0.35;
        let y = st.level_y(item.get(4));
        let color = item_color(item);
        let rot = item.get(9);

        // User-editable Code 9 term library (P5 = 3000..3069). These are
        // ordinary SCORE text strings stored in the SCORLIB CODE9.DAT file.
        if let Some(term) = term_library_text(p5) {
            let label = crate::model::decode_text(term.as_bytes());
            if !label.is_empty() {
                let size_pt = 10.0 * st.size * size;
                let shaped = self.fonts.shape_plain(&label, size_pt, 0, 1.0);
                let w = shaped.width;
                for p in self
                    .fonts
                    .text_run_paths(&shaped, size_pt, Pt::new(x, y), rot)
                {
                    self.draws.fill(p, color);
                }
                self.add_hit(
                    idx,
                    Pt::new(x, y - size_pt),
                    Pt::new(x + w, y + size_pt * 0.35),
                );
                return;
            }
        }

        // Dynamic / expression text families.
        if let Some(label) = symbol_text(p5) {
            let italic =
                matches!(p5, 50..=69 | 78..=86 | 111 | 113 | 114 | 123 | 127..=129 | 150..=169);
            let font = if italic { 2 } else { 0 };
            let size_pt = 11.0 * st.size * size;
            let shaped = self.fonts.shape_plain(&label, size_pt, font, 1.0);
            let w = shaped.width;
            let ox = x - w * 0.5;
            for p in self
                .fonts
                .text_run_paths(&shaped, size_pt, Pt::new(ox, y), rot)
            {
                self.draws.fill(p, color);
            }
            self.add_hit(
                idx,
                Pt::new(ox, y - size_pt),
                Pt::new(ox + w, y + size_pt * 0.4),
            );
            return;
        }

        let glyph = match p5 {
            0 => G_TREBLE,
            1 => G_BASS,
            2 => G_ALTO,
            3 => G_TAB,
            4 => G_PERCUSSION,
            6 => G_FLAT,
            7 => G_SHARP,
            8 => G_NATURAL,
            9 => G_DOUBLE_FLAT,
            10 => G_DOUBLE_SHARP,
            11 => G_NOTEHEAD_BLACK,
            12 => G_NOTEHEAD_HALF,
            13 => G_NOTEHEAD_WHOLE,
            14 => G_NOTEHEAD_BREVE,
            15 => G_NOTEHEAD_DIAMOND_WHITE,
            16 => G_NOTEHEAD_DIAMOND_BLACK,
            17 => G_NOTEHEAD_X,
            22 | 26 => G_ACCENT,
            23 | 27 => G_TENUTO,
            24 => G_STACCATO,
            28 => G_DOWNBOW,
            29 => G_UPBOW,
            31 => G_FERMATA,
            34 => G_MORDENT,
            35 => G_INVERTED_MORDENT,
            37 => G_TRILL,
            39 => G_REST_WHOLE,
            40 => G_REST_BREVE,
            41 => G_REST_WHOLE,
            42 => G_REST_HALF,
            43 => G_REST_QUARTER,
            44 => G_REST_8TH,
            74 => G_SEGNO,
            75 => G_TURN,
            112 | 122 => G_CODA,
            _ => '\0',
        };
        if glyph != '\0' {
            let gsize = 4.0 * st.space * size;
            if let Some(path) = self.fonts.music_glyph_centered(glyph, gsize, x, y, rot) {
                self.draws.fill(path, color);
            }
            self.add_hit(
                idx,
                Pt::new(x - 12.0, y - 18.0),
                Pt::new(x + 12.0, y + 18.0),
            );
            return;
        }
        match p5 {
            83 | 88 | 89 => {
                let r = 3.5 * st.size * size;
                self.draws.stroke(circle_path(x, y, r), color, 0.8);
                if p5 == 89 {
                    self.draws.fill(circle_path(x, y, r * 0.75), color);
                }
            }
            96 => {
                self.draws
                    .line(Pt::new(x, y - 6.0), Pt::new(x, y + 6.0), color, 0.8);
                self.draws
                    .line(Pt::new(x, y - 6.0), Pt::new(x + 4.0, y - 6.0), color, 0.8);
                self.draws
                    .line(Pt::new(x, y + 6.0), Pt::new(x + 4.0, y + 6.0), color, 0.8);
            }
            _ => {}
        }
    }

    fn draw_box(&mut self, idx: usize, item: &Item) {
        let Some(st) = self.staves.get(&item.staff()).cloned() else {
            return;
        };
        let p5 = item.get(5).round() as i32;
        if p5 > 2 {
            return;
        }
        let x = st.x0 + item.get(3) * SCORE_UNIT;
        let y = st.level_y(item.get(4));
        let w = (item.get(6).abs().max(1.0)) * st.space;
        let h = {
            let v = item.get(7).abs();
            if v < 0.01 { w } else { v * st.space }
        };
        let color = item_color(item);
        let width = (item.get(8).abs().max(0.5) * 0.35).max(0.5);
        if p5 == 1 {
            let mut p = Path::new();
            let k = 0.5522848;
            let rx = w * 0.5;
            let ry = h * 0.5;
            p.push(PathCmd::MoveTo(Pt::new(x + rx, y)));
            p.push(PathCmd::CubicTo(
                Pt::new(x + rx, y - k * ry),
                Pt::new(x + k * rx, y - ry),
                Pt::new(x, y - ry),
            ));
            p.push(PathCmd::CubicTo(
                Pt::new(x - k * rx, y - ry),
                Pt::new(x - rx, y - k * ry),
                Pt::new(x - rx, y),
            ));
            p.push(PathCmd::CubicTo(
                Pt::new(x - rx, y + k * ry),
                Pt::new(x - k * rx, y + ry),
                Pt::new(x, y + ry),
            ));
            p.push(PathCmd::CubicTo(
                Pt::new(x + k * rx, y + ry),
                Pt::new(x + rx, y + k * ry),
                Pt::new(x + rx, y),
            ));
            p.push(PathCmd::Close);
            self.draws.stroke(p, color, width);
        } else {
            let x0 = x - w * 0.5;
            let y0 = y - h * 0.5;
            if p5 == 0 {
                self.draws.stroke_rect(x0, y0, w, h, color, width);
            } else {
                self.draws.dotted_line(
                    Pt::new(x0, y0),
                    Pt::new(x0 + w, y0),
                    color,
                    width,
                    3.0,
                    2.0,
                );
                self.draws.dotted_line(
                    Pt::new(x0 + w, y0),
                    Pt::new(x0 + w, y0 + h),
                    color,
                    width,
                    3.0,
                    2.0,
                );
                self.draws.dotted_line(
                    Pt::new(x0 + w, y0 + h),
                    Pt::new(x0, y0 + h),
                    color,
                    width,
                    3.0,
                    2.0,
                );
                self.draws.dotted_line(
                    Pt::new(x0, y0 + h),
                    Pt::new(x0, y0),
                    color,
                    width,
                    3.0,
                    2.0,
                );
            }
        }
        self.add_hit(
            idx,
            Pt::new(x - w * 0.5, y - h * 0.5),
            Pt::new(x + w * 0.5, y + h * 0.5),
        );
    }

    fn draw_plain_text(
        &mut self,
        text: &str,
        x: f32,
        baseline: f32,
        size: f32,
        font: i32,
        color: Color,
        rotate: f32,
    ) {
        let shaped = self.fonts.shape_plain(text, size, font, 1.0);
        for p in self
            .fonts
            .text_run_paths(&shaped, size, Pt::new(x, baseline), rotate)
        {
            self.draws.fill(p, color);
        }
    }

    fn add_hit(&mut self, item: usize, a: Pt, b: Pt) {
        self.hits.push(Hit {
            item,
            min: Pt::new(a.x.min(b.x), a.y.min(b.y)),
            max: Pt::new(a.x.max(b.x), a.y.max(b.y)),
        });
    }
}

#[derive(Debug, Clone)]
struct NoteInfo {
    staff: i32,
    x: f32,
    y: f32,
    level: f32,
    head_code: i32,
    up: bool,
    stemmed: bool,
    headless: bool,
    stem_extra: f32,
    flags: i32,
    dots: i32,
    accidental: i32,
    accidental_space: f32,
    acc_parens: bool,
    size: f32,
    color: Color,
    mark: f32,
    no_ledger: bool,
}

impl NoteInfo {
    fn new(_idx: usize, item: &Item, staves: &BTreeMap<i32, StaffGeo>) -> Option<Self> {
        let mut staff_id = item.staff();
        let above_below = item.get(12).round() as i32;
        if above_below == 1 {
            staff_id += 1;
        } else if above_below == 2 {
            staff_id -= 1;
        }
        let st = staves.get(&staff_id)?;
        let mut level = item.get(4);
        let mut size = item.get(15);
        if size.abs() < 0.001 {
            size = 1.0;
        }
        if level >= 100.0 {
            level -= 100.0;
            size *= 0.58;
        } else if level <= -100.0 {
            level += 100.0;
            size *= 0.58;
        }
        let p6 = item.get(6).round() as i32;
        let head_code = if p6 >= 10 && p6 < 100 { p6 % 10 } else { p6 };
        let p5 = item.get(5).abs();
        let stem_digit = (p5.round() as i32 / 10) % 10;
        let stemmed = (stem_digit == 1 || stem_digit == 2) && !matches!(head_code, 2 | 3 | 7);
        let up = stem_digit != 2;
        let accidental = (p5.round() as i32) % 10;
        let accidental_space = {
            let frac = p5.fract().abs();
            if frac > 0.0001 {
                if (frac * 100.0).round() as i32 / 10 == 9 {
                    -(frac * 100.0 - 90.0) / 10.0
                } else {
                    frac * 10.0
                }
            } else {
                0.0
            }
        };
        let headless = head_code == 7;
        let p9 = item.get(9);
        let p9i = p9.round() as i32;
        let dots = (p9i / 10).abs() % 10;
        let explicit_flags = p9i.abs() % 10;
        let flags = if explicit_flags > 0 {
            explicit_flags
        } else {
            match item.get(7) {
                d if d <= 0.07 => 4,
                d if d <= 0.13 => 3,
                d if d <= 0.26 => 2,
                d if d <= 0.51 => 1,
                _ => 0,
            }
        };
        let p8 = item.get(8);
        let mut stem_extra = p8;
        if stem_extra.abs() >= 100.0 {
            stem_extra = stem_extra.abs() - 100.0;
        }
        let x = st.x0 + item.p3() * SCORE_UNIT + item.get(10) * 0.055 * st.space;
        let y = st.level_y(level);
        let color = item_color(item);
        Some(Self {
            staff: staff_id,
            x,
            y,
            level,
            head_code,
            up,
            stemmed,
            headless,
            stem_extra,
            flags,
            dots,
            accidental,
            accidental_space,
            acc_parens: item.get(5).abs() >= 100.0,
            size,
            color,
            mark: item.get(11),
            no_ledger: item.get(7) <= -2.0 && item.get(7) > -4.0,
        })
    }

    fn stem_dx(&self, space: f32) -> f32 {
        if self.up { 0.58 * space } else { -0.58 * space }
    }

    fn head_x(&self, space: f32) -> f32 {
        self.x - self.stem_dx(space)
    }

    fn head_glyph(&self) -> Option<char> {
        Some(match self.head_code {
            1 => G_NOTEHEAD_HALF,
            2 => G_NOTEHEAD_WHOLE,
            3 => G_NOTEHEAD_BREVE,
            4 => G_NOTEHEAD_DIAMOND_WHITE,
            5 => G_NOTEHEAD_DIAMOND_BLACK,
            6 => G_NOTEHEAD_X,
            8 => G_NOTEHEAD_CIRCLE_X,
            9 => G_NOTEHEAD_CIRCLE_X,
            7 => return None,
            _ => G_NOTEHEAD_BLACK,
        })
    }
}

fn item_color(item: &Item) -> Color {
    let p26 = item.get(26);
    if p26.abs() < 0.00001 {
        return Color::INK;
    }
    if (0.0..=1.0).contains(&p26) {
        let v = (p26 * 255.0).round() as u8;
        return Color::rgb(v, v, v);
    }
    if p26 >= 100000.0 && p26 < 1000000.0 {
        let n = p26.round() as u32;
        let r = ((n / 10000) % 100) as u8;
        let g = ((n / 100) % 100) as u8;
        let b = (n % 100) as u8;
        return Color::rgb(
            (r as u16 * 255 / 99) as u8,
            (g as u16 * 255 / 99) as u8,
            (b as u16 * 255 / 99) as u8,
        );
    }
    Color::INK
}

fn text_font_size(item: &Item, staff_size: f32) -> f32 {
    let mut p6 = item.get(6).abs();
    if p6 >= 300.0 {
        p6 -= 300.0;
    } else if p6 >= 200.0 {
        p6 -= 200.0;
    } else if p6 >= 100.0 {
        p6 -= 100.0;
    }
    if p6 < 0.001 {
        p6 = 1.0;
    }
    let p7 = item.get(7).abs();
    let v = if p7 < 0.001 { p6 } else { p7 };
    (10.0 * staff_size * v).clamp(1.0, 500.0)
}

fn clef_glyph(p5: f32) -> Option<char> {
    if (p5 - 0.0).abs() < 0.001 {
        Some(G_TREBLE)
    } else if (p5 - 1.0).abs() < 0.001 {
        Some(G_BASS)
    } else if (p5 - 2.0).abs() < 0.001 {
        Some(G_ALTO)
    } else if (p5 - 3.0).abs() < 0.001 {
        Some(G_TENOR)
    } else if (p5 - 4.0).abs() < 0.001 {
        Some(G_PERCUSSION)
    } else if (p5 - 0.8).abs() < 0.01 || (p5 - 0.88).abs() < 0.01 || (p5 - 0.15).abs() < 0.01 {
        Some(G_TREBLE)
    } else if (p5 - 1.8).abs() < 0.01 {
        Some(G_BASS)
    } else {
        None
    }
}

fn accidental_glyph(acc: i32) -> Option<char> {
    Some(match acc {
        0 => return None,
        1 => G_FLAT,
        2 => G_SHARP,
        3 => G_NATURAL,
        4 => G_DOUBLE_FLAT,
        5 => G_DOUBLE_SHARP,
        6 => G_QUARTER_FLAT,
        7 => G_THREE_QUARTER_FLAT,
        8 => G_QUARTER_SHARP,
        9 => G_THREE_QUARTER_SHARP,
        _ => return None,
    })
}

fn key_signature_levels(clef: i32, count: i32) -> Vec<f32> {
    let sharps_treble = [11.0, 8.0, 12.0, 9.0, 6.0, 10.0, 7.0];
    let flats_treble = [7.0, 10.0, 6.0, 9.0, 5.0, 8.0, 4.0];
    let offset = match clef {
        1 => -2.0,
        2 => -1.0,
        3 => -3.0,
        _ => 0.0,
    };
    let n = count.unsigned_abs().min(7) as usize;
    let src = if count >= 0 {
        &sharps_treble[..]
    } else {
        &flats_treble[..]
    };
    let mut out: Vec<f32> = (0..n).map(|i| src[i] + offset).collect();
    if count.unsigned_abs() > 7 {
        for i in 0..((count.unsigned_abs() - 7).min(7) as usize) {
            out.push(src[i] + offset - 7.0);
        }
    }
    out
}

fn note_flag_count(item: &Item) -> i32 {
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

fn number_label(item: &Item) -> String {
    let p5 = item.get(5).round() as i32;
    let p17 = item.get(17).round() as i32;
    let p18 = item.get(18).round() as i32;
    let mut s = String::new();
    if (1001..=2626).contains(&p5) {
        let n = p5 - 1000;
        let upper = n <= 26;
        let idx = ((n - 1).rem_euclid(26)) as u8;
        let base = if upper { b'A' } else { b'a' };
        s.push((base + idx) as char);
        if p18 > 0 {
            if p18 >= 100 {
                let a = ((p18 / 100) % 100) as u8;
                let b = (p18 % 100) as u8;
                if a > 0 {
                    s.push((b'A' + a - 1) as char);
                }
                if b > 0 {
                    s.push((b'A' + b - 1) as char);
                }
            } else {
                s.push_str(&p18.to_string());
            }
        }
    } else if (1..=999).contains(&p5) && p17 > 0 {
        s.push_str(&p5.to_string());
        let c = if p17 > 100 {
            (b'a' + (p17 - 101) as u8) as char
        } else {
            (b'A' + (p17 - 1) as u8) as char
        };
        s.push(c);
    } else if p18 >= 1000 {
        s.push_str(&p18.to_string());
    } else if (p5 as f32 - item.get(5)).abs() < 0.0001 {
        s.push_str(&p5.to_string());
    } else {
        s.push_str(&format!("{}", item.get(5)));
    }
    s
}

fn term_library_text(p5: i32) -> Option<&'static str> {
    if !(3000..=3069).contains(&p5) {
        return None;
    }
    const DATA: &str = include_str!("../assets/CODE9.DAT");
    for line in DATA.lines() {
        let line = line.trim_end();
        if line.len() > 5 && line.as_bytes()[4] == b' ' {
            if let Ok(n) = line[0..4].parse::<i32>() {
                if n == p5 {
                    return Some(line[5..].trim_end());
                }
            }
        }
    }
    None
}

fn symbol_text(p5: i32) -> Option<String> {
    let s = match p5 {
        50 => "pppp",
        51 => "ppp",
        52 => "pp",
        53 => "p",
        54 => "mp",
        55 => "mf",
        56 => "f",
        57 => "ff",
        58 => "fff",
        59 => "ffff",
        60 => "fp",
        61 => "sf",
        62 => "sfz",
        63 => "cresc.",
        64 => "dim.",
        65 => "rit.",
        66 => "pizz.",
        67 => "arco",
        68 => "accel.",
        69 => "→",
        70 => "Ped.",
        71 => "*",
        72 => "gliss.",
        73 => "molto",
        76 => "//",
        77 => "//",
        78 => "a tempo",
        79 => "espr.",
        80 => "meno",
        81 => "più",
        82 => "snap",
        85 => "subito",
        86 => "poco",
        87 => "grid",
        90 => "×",
        91 => "()",
        111 => "decresc.",
        113 => "rall.",
        114 => "mosso",
        115 => "𝄏",
        116 => "𝄏",
        117 => "𝄐",
        118 => ",",
        120 => "P",
        121 => "𝄋",
        123 => "fz",
        127 => "rf",
        128 => "rfz",
        129 => "sffz",
        150 => "(pppp)",
        151 => "(ppp)",
        152 => "(pp)",
        153 => "(p)",
        154 => "(mp)",
        155 => "(mf)",
        156 => "(f)",
        157 => "(ff)",
        158 => "(fff)",
        _ => return None,
    };
    Some(s.to_string())
}

fn circle_path(cx: f32, cy: f32, r: f32) -> Path {
    let k = 0.5522848;
    let mut p = Path::new();
    p.push(PathCmd::MoveTo(Pt::new(cx + r, cy)));
    p.push(PathCmd::CubicTo(
        Pt::new(cx + r, cy - k * r),
        Pt::new(cx + k * r, cy - r),
        Pt::new(cx, cy - r),
    ));
    p.push(PathCmd::CubicTo(
        Pt::new(cx - k * r, cy - r),
        Pt::new(cx - r, cy - k * r),
        Pt::new(cx - r, cy),
    ));
    p.push(PathCmd::CubicTo(
        Pt::new(cx - r, cy + k * r),
        Pt::new(cx - k * r, cy + r),
        Pt::new(cx, cy + r),
    ));
    p.push(PathCmd::CubicTo(
        Pt::new(cx + k * r, cy + r),
        Pt::new(cx + r, cy + k * r),
        Pt::new(cx + r, cy),
    ));
    p.push(PathCmd::Close);
    p
}

/// Draw a tasteful paper backdrop with a page edge.  Used only on screen.
pub fn page_shadow(draws: &mut DrawList, page: &EngravedPage) {
    draws.stroke_rect(
        -0.8,
        -0.8,
        page.width + 1.6,
        page.height + 1.6,
        Color::LIGHT_GRAY,
        0.6,
    );
}
