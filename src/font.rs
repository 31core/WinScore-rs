//! Font loading, text shaping and outline extraction.
//!
//! The engraver turns every glyph into vector outlines.  This guarantees that
//! the screen preview, PNG, SVG and PDF exports are metrically identical and
//! that exported PDFs need no font embedding.

use owned_ttf_parser::{AsFaceRef, Face, GlyphId, OutlineBuilder, OwnedFace};

use crate::draw::{Path, PathCmd, Pt};
use crate::model;

pub const MUSIC_FONT: usize = 4;

pub const G_TREBLE: char = '\u{E050}';
pub const G_BASS: char = '\u{E062}';
pub const G_ALTO: char = '\u{E05C}';
pub const G_TENOR: char = '\u{E05D}';
pub const G_PERCUSSION: char = '\u{E069}';
pub const G_TAB: char = '\u{E06A}';

pub const G_FLAT: char = '\u{E260}';
pub const G_NATURAL: char = '\u{E261}';
pub const G_SHARP: char = '\u{E262}';
pub const G_DOUBLE_FLAT: char = '\u{E264}';
pub const G_DOUBLE_SHARP: char = '\u{E263}';
pub const G_QUARTER_FLAT: char = '\u{E275}';
pub const G_QUARTER_SHARP: char = '\u{E276}';
pub const G_THREE_QUARTER_FLAT: char = '\u{E277}';
pub const G_THREE_QUARTER_SHARP: char = '\u{E278}';

pub const G_NOTEHEAD_BREVE: char = '\u{E0A0}';
pub const G_NOTEHEAD_WHOLE: char = '\u{E0A2}';
pub const G_NOTEHEAD_HALF: char = '\u{E0A3}';
pub const G_NOTEHEAD_BLACK: char = '\u{E0A4}';
pub const G_NOTEHEAD_X: char = '\u{E0A9}';
pub const G_NOTEHEAD_DIAMOND_WHITE: char = '\u{E0B2}';
pub const G_NOTEHEAD_DIAMOND_BLACK: char = '\u{E0B3}';
pub const G_NOTEHEAD_CIRCLE_X: char = '\u{E0B6}';
pub const G_NOTEHEAD_PAREN: char = '\u{E0F6}';

pub const G_REST_BREVE: char = '\u{E4E2}';
pub const G_REST_WHOLE: char = '\u{E4E3}';
pub const G_REST_HALF: char = '\u{E4E4}';
pub const G_REST_QUARTER: char = '\u{E4E5}';
pub const G_REST_8TH: char = '\u{E4E6}';
pub const G_REST_16TH: char = '\u{E4E7}';
pub const G_REST_32ND: char = '\u{E4E8}';
pub const G_REST_64TH: char = '\u{E4E9}';

pub const G_ACCENT: char = '\u{E4A0}';
pub const G_STACCATO: char = '\u{E4A2}';
pub const G_TENUTO: char = '\u{E4A4}';
pub const G_MARCATO: char = '\u{E4AC}';
pub const G_FERMATA: char = '\u{E4C0}';
pub const G_DOWNBOW: char = '\u{E610}';
pub const G_UPBOW: char = '\u{E612}';
pub const G_MORDENT: char = '\u{E56C}';
pub const G_INVERTED_MORDENT: char = '\u{E56D}';
pub const G_TRILL: char = '\u{E566}';
pub const G_TURN: char = '\u{E567}';
pub const G_SEGNO: char = '\u{E047}';
pub const G_CODA: char = '\u{E048}';
pub const G_PLUS: char = '\u{E08A}';
pub const G_TIME_COMMON: char = '\u{E08A}';
pub const G_TIME_CUT: char = '\u{E08B}';
pub const G_BRACE: char = '\u{E000}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

impl FontKind {
    pub fn index(self) -> usize {
        self as usize
    }
}

pub struct FontBook {
    faces: Vec<OwnedFace>,
    pub music_index: usize,
    pub loaded_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlacedGlyph {
    pub face: usize,
    pub glyph: GlyphId,
    pub ch: char,
    pub x: f32,
    pub advance: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ShapedText {
    pub glyphs: Vec<PlacedGlyph>,
    pub width: f32,
}

impl FontBook {
    pub fn load() -> Self {
        let mut faces = Vec::new();
        let mut names = Vec::new();

        fn add_one(
            faces: &mut Vec<OwnedFace>,
            names: &mut Vec<String>,
            bytes: Vec<u8>,
            name: &str,
        ) {
            match OwnedFace::from_vec(bytes, 0) {
                Ok(face) => {
                    faces.push(face);
                    names.push(name.to_string());
                }
                Err(err) => eprintln!("could not parse font {name}: {err}"),
            }
        }

        // Order must match FontKind::Regular .. BoldItalic.
        let candidates = [
            (
                "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
                "assets/fonts/LiberationSerif-Regular.ttf",
                "Times New Roman",
            ),
            (
                "/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf",
                "assets/fonts/LiberationSerif-Bold.ttf",
                "Times New Roman Bold",
            ),
            (
                "/System/Library/Fonts/Supplemental/Times New Roman Italic.ttf",
                "assets/fonts/LiberationSerif-Italic.ttf",
                "Times New Roman Italic",
            ),
            (
                "/System/Library/Fonts/Supplemental/Times New Roman Bold Italic.ttf",
                "assets/fonts/LiberationSerif-BoldItalic.ttf",
                "Times New Roman Bold Italic",
            ),
        ];
        for (system, bundled, name) in candidates {
            if let Ok(bytes) = std::fs::read(system) {
                add_one(&mut faces, &mut names, bytes, name);
            } else if let Ok(bytes) = std::fs::read(bundled) {
                add_one(
                    &mut faces,
                    &mut names,
                    bytes,
                    &format!("{name} (Liberation fallback)"),
                );
            }
        }
        while faces.len() < 4 {
            // Last-resort fallback so the app always starts.
            add_one(
                &mut faces,
                &mut names,
                include_bytes!("../assets/fonts/LiberationSerif-Regular.ttf").to_vec(),
                "Liberation Serif",
            );
        }

        add_one(
            &mut faces,
            &mut names,
            include_bytes!("../assets/fonts/Bravura.otf").to_vec(),
            "Bravura",
        );
        let music_index = faces.len() - 1;

        Self {
            faces,
            music_index,
            loaded_names: names,
        }
    }

    pub fn face(&self, idx: usize) -> &Face<'_> {
        self.faces.get(idx).unwrap_or(&self.faces[0]).as_face_ref()
    }

    pub fn font_kind(font_number: i32) -> FontKind {
        match font_number {
            28 => FontKind::Italic,
            0 | 4 | 8 | 12 | 16 | 20 | 24 | 29 | 35 | 38 | 41 | 44 | 47 | 50 | 51 | 53 => {
                FontKind::Regular
            }
            1 | 5 | 9 | 13 | 17 | 21 | 25 | 30 | 36 | 39 | 42 | 45 | 48 => FontKind::Bold,
            2 | 6 | 10 | 14 | 18 | 22 | 26 | 31 | 37 | 40 | 43 | 46 | 49 => FontKind::Italic,
            3 | 7 | 11 | 15 | 19 | 23 | 27 | 32 => FontKind::BoldItalic,
            _ => FontKind::Regular,
        }
    }

    fn choose_face(&self, font_number: i32) -> usize {
        // P8 of a text object is an override expressed as 1000 + font number.
        let n = if font_number >= 1000 {
            font_number - 1000
        } else {
            font_number
        };
        Self::font_kind(n).index().min(3)
    }

    pub fn music_face(&self) -> &Face<'_> {
        self.face(self.music_index)
    }

    pub fn shape(
        &self,
        raw: &[u8],
        size: f32,
        tracking: f32,
        space_factor: f32,
        font_override: Option<i32>,
    ) -> ShapedText {
        let runs = model::text_runs(raw);
        let mut out = ShapedText::default();
        let mut pen = 0.0f32;
        let tracking = if tracking.abs() < 0.001 {
            1.0
        } else {
            tracking
        };
        let space_factor = if space_factor.abs() < 0.001 {
            1.0
        } else {
            space_factor
        };

        for (font_num, text) in runs {
            let num = font_override.unwrap_or(font_num as i32);
            let face_idx = self.choose_face(num);
            let face = self.face(face_idx);
            let upem = face.units_per_em().max(1) as f32;
            for ch in text.chars() {
                let gid = face
                    .glyph_index(ch)
                    .or_else(|| face.glyph_index('?'))
                    .unwrap_or(GlyphId(0));
                let mut adv = face.glyph_hor_advance(gid).unwrap_or(0) as f32 / upem * size;
                if ch == ' ' {
                    adv *= space_factor;
                }
                adv *= tracking;
                out.glyphs.push(PlacedGlyph {
                    face: face_idx,
                    glyph: gid,
                    ch,
                    x: pen,
                    advance: adv,
                });
                pen += adv;
            }
        }
        out.width = pen;
        out
    }

    pub fn shape_plain(
        &self,
        text: &str,
        size: f32,
        font_number: i32,
        tracking: f32,
    ) -> ShapedText {
        let face_idx = self.choose_face(font_number);
        let face = self.face(face_idx);
        let upem = face.units_per_em().max(1) as f32;
        let tracking = if tracking.abs() < 0.001 {
            1.0
        } else {
            tracking
        };
        let mut out = ShapedText::default();
        let mut pen = 0.0f32;
        for ch in text.chars() {
            let gid = face
                .glyph_index(ch)
                .or_else(|| face.glyph_index('?'))
                .unwrap_or(GlyphId(0));
            let mut adv = face.glyph_hor_advance(gid).unwrap_or(0) as f32 / upem * size;
            adv *= tracking;
            out.glyphs.push(PlacedGlyph {
                face: face_idx,
                glyph: gid,
                ch,
                x: pen,
                advance: adv,
            });
            pen += adv;
        }
        out.width = pen;
        out
    }

    /// Natural width of a run, used by the formatting commands.
    pub fn text_width(&self, raw: &[u8], size: f32, tracking: f32, space_factor: f32) -> f32 {
        self.shape(raw, size, tracking, space_factor, None).width
    }

    pub fn glyph_path(
        &self,
        face_idx: usize,
        glyph: GlyphId,
        size: f32,
        origin: Pt,
    ) -> Option<Path> {
        let face = self.face(face_idx);
        if glyph.0 == 0 {
            return None;
        }
        let upem = face.units_per_em().max(1) as f32;
        let scale = size / upem;
        let mut builder = OutlineToPath {
            path: Path::new(),
            ox: origin.x,
            oy: origin.y,
            scale,
        };
        face.outline_glyph(glyph, &mut builder)?;
        if builder.path.is_empty() {
            None
        } else {
            Some(builder.path)
        }
    }

    /// A music symbol positioned by the centre of its ink bounding box.
    pub fn music_glyph_centered(
        &self,
        c: char,
        size: f32,
        cx: f32,
        cy: f32,
        rotate: f32,
    ) -> Option<Path> {
        let face = self.music_face();
        let gid = face.glyph_index(c)?;
        let p = self.glyph_path(self.music_index, gid, size, Pt::new(0.0, 0.0))?;
        if p.is_empty() {
            return None;
        }
        let bx = (p.min.x + p.max.x) * 0.5;
        let by = (p.min.y + p.max.y) * 0.5;
        let centered = p.translate(-bx, -by);
        let rotated = if rotate.abs() > 0.001 {
            centered.transform(0.0, 0.0, rotate)
        } else {
            centered
        };
        Some(rotated.translate(cx, cy))
    }

    /// Music symbol positioned by its baseline rather than its ink centre.
    pub fn music_glyph_baseline(
        &self,
        c: char,
        size: f32,
        x: f32,
        baseline_y: f32,
        rotate: f32,
    ) -> Option<Path> {
        let face = self.music_face();
        let gid = face.glyph_index(c)?;
        let p = self.glyph_path(self.music_index, gid, size, Pt::new(x, baseline_y))?;
        if rotate.abs() > 0.001 {
            Some(p.transform(0.0, 0.0, rotate))
        } else {
            Some(p)
        }
    }

    pub fn text_run_paths(
        &self,
        shaped: &ShapedText,
        size: f32,
        origin: Pt,
        rotate_deg: f32,
    ) -> Vec<Path> {
        let mut paths = Vec::new();
        let a = rotate_deg.to_radians();
        let (s, c) = a.sin_cos();
        for g in &shaped.glyphs {
            if g.ch == ' ' {
                continue;
            }
            let p = Pt::new(origin.x + g.x, origin.y);
            if let Some(mut path) = self.glyph_path(g.face, g.glyph, size, p) {
                if rotate_deg.abs() > 0.001 {
                    // Rotate the glyph outline around the run origin.
                    let mut cmds = Vec::with_capacity(path.cmds.len());
                    for cmd in path.cmds.drain(..) {
                        let tf = |q: Pt| {
                            let dx = q.x - origin.x;
                            let dy = q.y - origin.y;
                            Pt::new(origin.x + dx * c - dy * s, origin.y + dx * s + dy * c)
                        };
                        cmds.push(match cmd {
                            PathCmd::MoveTo(q) => PathCmd::MoveTo(tf(q)),
                            PathCmd::LineTo(q) => PathCmd::LineTo(tf(q)),
                            PathCmd::QuadTo(a, b) => PathCmd::QuadTo(tf(a), tf(b)),
                            PathCmd::CubicTo(a, b, d) => PathCmd::CubicTo(tf(a), tf(b), tf(d)),
                            PathCmd::Close => PathCmd::Close,
                        });
                    }
                    path = Path::from_cmds(cmds);
                }
                paths.push(path);
            }
        }
        paths
    }

    pub fn ascent(&self, face_idx: usize, size: f32) -> f32 {
        let face = self.face(face_idx);
        face.ascender() as f32 / face.units_per_em().max(1) as f32 * size
    }
}

struct OutlineToPath {
    path: Path,
    ox: f32,
    oy: f32,
    scale: f32,
}

impl OutlineBuilder for OutlineToPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.push(PathCmd::MoveTo(Pt::new(
            self.ox + x * self.scale,
            self.oy - y * self.scale,
        )));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.push(PathCmd::LineTo(Pt::new(
            self.ox + x * self.scale,
            self.oy - y * self.scale,
        )));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.push(PathCmd::QuadTo(
            Pt::new(self.ox + x1 * self.scale, self.oy - y1 * self.scale),
            Pt::new(self.ox + x * self.scale, self.oy - y * self.scale),
        ));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.push(PathCmd::CubicTo(
            Pt::new(self.ox + x1 * self.scale, self.oy - y1 * self.scale),
            Pt::new(self.ox + x2 * self.scale, self.oy - y2 * self.scale),
            Pt::new(self.ox + x * self.scale, self.oy - y * self.scale),
        ));
    }

    fn close(&mut self) {
        self.path.push(PathCmd::Close);
    }
}
