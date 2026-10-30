//! The WinScore desktop application: a deliberately old-fashioned, colourful
//! editor built around the classic SCORE parameter model.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::draw::Pt;
use crate::engrave::{EngravedPage, Engraver, Hit, PAGE_H, PAGE_W, SCORE_UNIT};
use crate::font::FontBook;
use crate::model::{Item, Score, TextBlock};
use crate::{pdf, raster, svg};

const APP_NAME: &str = "WinScore 5.01 — Rust Edition";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tool {
    Select,
    Note { head: i32, dur: f32 },
    Rest { p5: i32, dur: f32 },
    Clef(i32),
    Meter(i32, i32),
    Key(i32),
    Barline(i32),
    Dynamic(i32),
    Text,
    Slur,
    Beam,
    Hairpin,
    Number,
    Trill,
    Ottava(i32),
    Pedal(i32),
    Ending(i32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    Page,
    Width,
}

#[derive(Debug, Clone)]
struct DragState {
    item: usize,
    origin: Pos2,
    p3: f32,
    p4: f32,
    staff: i32,
    snapshot: Score,
}

#[derive(Debug, Clone)]
enum TextTarget {
    Existing(usize),
    New { staff: i32, p3: f32, p4: f32 },
}

#[derive(Debug, Clone)]
struct StaffForm {
    staff: i32,
    p3: f32,
    p4: f32,
    p5: f32,
    p6: f32,
    lines: i32,
    instrument: i32,
    p10: f32,
}

pub struct WinScoreApp {
    score: Score,
    fonts: FontBook,
    page: EngravedPage,
    texture: Option<egui::TextureHandle>,
    tex_scale: f32,
    zoom: f32,
    fit_request: Option<FitMode>,
    selected: Option<usize>,
    hovered: Option<usize>,
    dirty: bool,
    status: String,
    command: String,
    undo: Vec<Score>,
    redo: Vec<Score>,
    recent: Vec<PathBuf>,
    tool: Tool,
    accidental: i32,
    dotted: bool,
    show_palette: bool,
    show_help: bool,
    show_about: bool,
    show_text_editor: bool,
    show_staff_editor: bool,
    staff_form: StaffForm,
    text_buf: String,
    text_target: Option<TextTarget>,
    text_font_prefix: String,
    text_size: f32,
    drag: Option<DragState>,
    last_canvas: Vec2,
    pending_scroll: Option<f32>,
    copied: Option<Item>,
    last_dir: PathBuf,
}

impl WinScoreApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        let fonts = FontBook::load();
        let mut opened_path: Option<PathBuf> = None;
        let mut score = if let Some(path) = initial {
            match Score::load(&path) {
                Ok(s) => {
                    opened_path = Some(path.clone());
                    s
                }
                Err(err) => {
                    eprintln!("could not load {path:?}: {err}");
                    Score::parse(include_bytes!("../samples/mozart.mus")).unwrap()
                }
            }
        } else {
            // A quietly included demonstration makes the first launch useful.
            Score::parse(include_bytes!("../samples/mozart.mus")).unwrap()
        };
        if opened_path.is_none() {
            score.path = None;
        }
        let recent: Vec<PathBuf> = opened_path.iter().cloned().collect();
        let start_dir = opened_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                if cwd == Path::new("/") {
                    std::env::var_os("HOME").map(PathBuf::from).unwrap_or(cwd)
                } else {
                    cwd
                }
            });

        let mut app = Self {
            score,
            fonts,
            page: EngravedPage {
                draws: Default::default(),
                hits: Vec::new(),
                staves: Default::default(),
                width: PAGE_W,
                height: PAGE_H,
            },
            texture: None,
            tex_scale: 1.5,
            zoom: 0.72,
            fit_request: Some(FitMode::Page),
            selected: None,
            hovered: None,
            dirty: false,
            status: "For Help, press F1.  Click a note to edit, or choose a tool below.".into(),
            command: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            recent,
            tool: Tool::Select,
            accidental: 0,
            dotted: false,
            show_palette: false,
            show_help: false,
            show_about: false,
            show_text_editor: false,
            show_staff_editor: false,
            staff_form: StaffForm {
                staff: 1,
                p3: 0.0,
                p4: 0.0,
                p5: 0.9,
                p6: 200.0,
                lines: 5,
                instrument: 0,
                p10: 0.0,
            },
            text_buf: String::new(),
            text_target: None,
            text_font_prefix: "_00".into(),
            text_size: 12.0,
            drag: None,
            last_canvas: Vec2::new(900.0, 600.0),
            pending_scroll: None,
            copied: None,
            last_dir: start_dir,
        };
        configure_style(&cc.egui_ctx);
        app.rebuild();
        if let Ok(v) = std::env::var("WINSCORE_ZOOM") {
            if let Ok(z) = v.parse::<f32>() {
                app.zoom = z.clamp(0.25, 3.0);
                app.fit_request = None;
            }
        }
        if std::env::var_os("WINSCORE_PALETTE").is_some() {
            app.show_palette = true;
        }
        if std::env::var_os("WINSCORE_STAFF").is_some() {
            app.open_staff_editor();
        }
        if let Ok(v) = std::env::var("WINSCORE_SELECT") {
            if let Ok(n) = v.parse::<usize>() {
                if n >= 1 && n <= app.score.items.len() {
                    app.selected = Some(n - 1);
                    app.status = format!("Item {} — {}", n, app.score.items[n - 1].label());
                }
            }
        }
        if let Ok(cmd) = std::env::var("WINSCORE_CMD") {
            app.execute_command(&cmd);
        }
        app
    }

    fn rebuild(&mut self) {
        self.page = Engraver::new(&self.score, &self.fonts).engrave();
        self.texture = None;
    }

    fn mark_changed(&mut self) {
        self.dirty = true;
        self.rebuild();
    }

    fn push_undo(&mut self) {
        self.undo.push(self.score.clone());
        if self.undo.len() > 120 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn do_undo(&mut self) {
        if let Some(mut s) = self.undo.pop() {
            self.redo.push(self.score.clone());
            let path = self.score.path.clone();
            s.path = path;
            self.score = s;
            self.selected = None;
            self.dirty = true;
            self.rebuild();
            self.status = "Undo.".into();
        } else {
            self.status = "Nothing to undo.".into();
        }
    }

    fn do_redo(&mut self) {
        if let Some(mut s) = self.redo.pop() {
            self.undo.push(self.score.clone());
            let path = self.score.path.clone();
            s.path = path;
            self.score = s;
            self.selected = None;
            self.dirty = true;
            self.rebuild();
            self.status = "Redo.".into();
        } else {
            self.status = "Nothing to redo.".into();
        }
    }

    fn select_at(&mut self, page_pos: Pos2) -> Option<usize> {
        let p = Pt::new(page_pos.x, page_pos.y);
        let mut best: Option<(&Hit, f32)> = None;
        for hit in &self.page.hits {
            if hit.contains(p) {
                let area = (hit.max.x - hit.min.x) * (hit.max.y - hit.min.y);
                if best.map(|(_, a)| area < a).unwrap_or(true) {
                    best = Some((hit, area));
                }
            }
        }
        if let Some((hit, _)) = best {
            self.selected = Some(hit.item);
            self.status = format!(
                "Item {} — {} (P2 staff {}, P3 {:.2}, P4 {:.2})",
                hit.item + 1,
                self.score.items[hit.item].label(),
                self.score.items[hit.item].staff(),
                self.score.items[hit.item].p3(),
                self.score.items[hit.item].p4()
            );
            Some(hit.item)
        } else {
            None
        }
    }

    fn hit_at(&self, page_pos: Pos2) -> Option<Hit> {
        let p = Pt::new(page_pos.x, page_pos.y);
        self.page
            .hits
            .iter()
            .filter(|h| h.contains(p))
            .min_by(|a, b| {
                let aa = (a.max.x - a.min.x) * (a.max.y - a.min.y);
                let ba = (b.max.x - b.min.x) * (b.max.y - b.min.y);
                aa.partial_cmp(&ba).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    fn staff_at(&self, page_y: f32) -> Option<i32> {
        self.page
            .staves
            .values()
            .min_by(|a, b| {
                let da = if page_y < a.top {
                    a.top - page_y
                } else if page_y > a.bottom {
                    page_y - a.bottom
                } else {
                    0.0
                };
                let db = if page_y < b.top {
                    b.top - page_y
                } else if page_y > b.bottom {
                    page_y - b.bottom
                } else {
                    0.0
                };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.id)
    }

    fn level_at(&self, staff: i32, page_y: f32) -> f32 {
        if let Some(st) = self.page.staves.get(&staff) {
            let raw = 3.0 + (st.bottom - page_y) / (st.space * 0.5);
            (raw * 2.0).round() / 2.0
        } else {
            7.0
        }
    }

    fn x_to_p3(&self, page_x: f32) -> f32 {
        let v = (page_x - crate::engrave::PAGE_LEFT) / SCORE_UNIT;
        (v * 20.0).round() / 20.0
    }

    fn add_tool_at(&mut self, tool: Tool, page_pos: Pos2) {
        let staff = self.staff_at(page_pos.y).unwrap_or(1);
        let p3 = self.x_to_p3(page_pos.x).clamp(0.0, 200.0);
        let mut level = self.level_at(staff, page_pos.y);
        if !(level.is_finite()) {
            level = 7.0;
        }
        self.push_undo();
        let idx = match tool {
            Tool::Select => return,
            Tool::Note { head, dur } => {
                let dur = if self.dotted { dur * 1.5 } else { dur };
                let stem = if head == 2 || head == 3 {
                    self.accidental as f32
                } else if level >= 7.0 {
                    20.0 + self.accidental as f32
                } else {
                    10.0 + self.accidental as f32
                };
                self.score.add_item(Item::new(
                    1,
                    vec![
                        1.0,
                        staff as f32,
                        p3,
                        level,
                        stem,
                        head as f32,
                        dur,
                        0.0,
                        if self.dotted { 10.0 } else { 0.0 },
                    ],
                ))
            }
            Tool::Rest { p5, dur } => {
                let dur = if self.dotted { dur * 1.5 } else { dur };
                self.score.add_item(Item::new(
                    2,
                    vec![
                        2.0,
                        staff as f32,
                        p3,
                        0.0,
                        p5 as f32,
                        if self.dotted { 1.0 } else { 0.0 },
                        dur,
                    ],
                ))
            }
            Tool::Clef(kind) => self
                .score
                .add_item(Item::new(3, vec![3.0, staff as f32, p3, 0.0, kind as f32])),
            Tool::Meter(top, bottom) => self.score.add_item(Item::new(
                18,
                vec![18.0, staff as f32, p3, 0.0, top as f32, bottom as f32],
            )),
            Tool::Key(n) => self.score.add_item(Item::new(
                17,
                vec![17.0, staff as f32, p3, 0.0, n as f32, 0.0, 1.0],
            )),
            Tool::Barline(kind) => self.score.add_item(Item::new(
                14,
                vec![14.0, staff as f32, p3, 1.0, kind as f32],
            )),
            Tool::Dynamic(sym) => self.score.add_item(Item::new(
                9,
                vec![9.0, staff as f32, p3, level, sym as f32, 1.0],
            )),
            Tool::Slur => self.score.add_item(Item::new(
                5,
                vec![
                    5.0,
                    staff as f32,
                    p3,
                    level,
                    level,
                    p3 + 10.0,
                    -2.0,
                    -1.0,
                    0.0,
                ],
            )),
            Tool::Beam => self.score.add_item(Item::new(
                6,
                vec![6.0, staff as f32, p3, level, level, p3 + 10.0, 11.0],
            )),
            Tool::Hairpin => self.score.add_item(Item::new(
                4,
                vec![4.0, staff as f32, p3, level, 999.0, p3 + 12.0, 0.0],
            )),
            Tool::Number => self
                .score
                .add_item(Item::new(10, vec![10.0, staff as f32, p3, level, 1.0, 1.0])),
            Tool::Trill => self.score.add_item(Item::new(
                7,
                vec![7.0, staff as f32, p3, 13.0, 0.0, p3 + 12.0, 0.0],
            )),
            Tool::Ottava(kind) => self.score.add_item(Item::new(
                7,
                vec![7.0, staff as f32, p3, 15.0, 0.0, p3 + 12.0, kind as f32],
            )),
            Tool::Pedal(kind) => self.score.add_item(Item::new(
                7,
                vec![7.0, staff as f32, p3, 1.0, 0.0, p3 + 8.0, kind as f32],
            )),
            Tool::Ending(number) => self.score.add_item(Item::new(
                5,
                vec![
                    5.0,
                    staff as f32,
                    p3,
                    13.0,
                    13.0,
                    p3 + 16.0,
                    -2.0,
                    number as f32,
                    0.0,
                ],
            )),
            Tool::Text => {
                self.text_target = Some(TextTarget::New {
                    staff,
                    p3,
                    p4: level,
                });
                self.text_buf = String::new();
                self.text_font_prefix = "_00".into();
                self.text_size = 12.0;
                self.show_text_editor = true;
                self.tool = Tool::Select;
                return;
            }
        };
        self.selected = Some(idx);
        self.mark_changed();
        self.status = format!("Added {}.", self.score.items[idx].label());
    }

    fn open_path(&mut self, path: PathBuf) {
        match Score::load(&path) {
            Ok(score) => {
                self.score = score;
                self.selected = None;
                self.undo.clear();
                self.redo.clear();
                self.dirty = false;
                self.recent.retain(|p| p != &path);
                self.recent.insert(0, path.clone());
                self.recent.truncate(10);
                if let Some(parent) = path.parent() {
                    self.last_dir = parent.to_path_buf();
                }
                self.fit_request = Some(FitMode::Page);
                self.rebuild();
                self.status = format!("Read {}.", path.display());
            }
            Err(err) => {
                self.status = format!("Could not read {}: {err}", path.display());
            }
        }
    }

    fn resolve_path(&self, name: &str) -> PathBuf {
        let mut p = PathBuf::from(name);
        if p.extension().is_none() {
            p.set_extension("mus");
        }
        if p.is_absolute() {
            p
        } else if let Some(parent) = self.score.path.as_ref().and_then(|p| p.parent()) {
            parent.join(p)
        } else {
            self.last_dir.join(p)
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        match self.score.save(&path) {
            Ok(()) => {
                self.score.path = Some(path.clone());
                self.dirty = false;
                self.recent.retain(|p| p != &path);
                self.recent.insert(0, path.clone());
                self.recent.truncate(10);
                self.status = format!("Saved {}.", path.display());
            }
            Err(err) => self.status = format!("Could not save {}: {err}", path.display()),
        }
    }

    fn save_same(&mut self) {
        if let Some(p) = self.score.path.clone() {
            self.save_to(p);
        } else {
            self.save_as_dialog();
        }
    }

    fn save_as_dialog(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("WinScore music", &["mus", "MUS"])
            .set_directory(&self.last_dir)
            .set_file_name(
                self.score
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("UNTITLED.MUS"),
            );
        if let Some(path) = dialog.save_file() {
            self.save_to(path);
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WinScore music", &["mus", "MUS"])
            .set_directory(&self.last_dir)
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn export(&mut self, ext: &str) {
        let default_name = self
            .score
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| format!("{}.{ext}", s.to_string_lossy()))
            .unwrap_or_else(|| format!("WINSCORE.{ext}"));
        let path = rfd::FileDialog::new()
            .set_directory(&self.last_dir)
            .set_file_name(default_name)
            .save_file();
        let Some(path) = path else { return };
        let result: anyhow::Result<()> = (|| match ext {
            "pdf" => {
                std::fs::write(
                    &path,
                    pdf::to_pdf(&self.page.draws, self.page.width, self.page.height),
                )?;
                Ok(())
            }
            "svg" => {
                std::fs::write(
                    &path,
                    svg::to_svg(&self.page.draws, self.page.width, self.page.height),
                )?;
                Ok(())
            }
            "png" => {
                let bytes = raster::render_png(
                    &self.page.draws,
                    self.page.width,
                    self.page.height,
                    300.0 / 72.0,
                )?;
                std::fs::write(&path, bytes)?;
                Ok(())
            }
            _ => Ok(()),
        })();
        match result {
            Ok(()) => self.status = format!("Exported {}.", path.display()),
            Err(err) => self.status = format!("Export failed: {err}"),
        }
    }

    fn new_score(&mut self) {
        self.push_undo();
        let mut score = Score::default();
        score
            .items
            .push(Item::new(8, vec![8.0, 1.0, 0.0, 0.0, 1.0, 200.0]));
        score
            .items
            .push(Item::new(3, vec![3.0, 1.0, 2.0, 0.0, 0.0]));
        score
            .items
            .push(Item::new(18, vec![18.0, 1.0, 8.0, 0.0, 4.0, 4.0]));
        self.score = score;
        self.selected = None;
        self.dirty = true;
        self.tool = Tool::Note { head: 0, dur: 1.0 };
        self.fit_request = Some(FitMode::Page);
        self.rebuild();
        self.status = "New page.  Choose a note value and click on the staff.".into();
    }

    fn delete_selected(&mut self) {
        if let Some(idx) = self.selected {
            if idx < self.score.items.len() {
                self.push_undo();
                let removed = self.score.items.remove(idx);
                self.copied = Some(removed);
                self.selected = None;
                self.mark_changed();
                self.status = "Item deleted.  Use UNDO to restore.".into();
            }
        }
    }

    fn move_selected(&mut self, dx: f32, dy: f32) {
        if let Some(idx) = self.selected {
            self.push_undo();
            let item = &mut self.score.items[idx];
            item.set(3, item.p3() + dx);
            item.set(4, item.p4() + dy);
            self.mark_changed();
            let item = &self.score.items[idx];
            self.status = format!("Moved to P3 {:.2}, P4 {:.2}.", item.p3(), item.p4());
        }
    }

    fn execute_command(&mut self, input: &str) {
        let line = input.trim().to_string();
        if line.is_empty() {
            return;
        }
        self.command.clear();
        self.status = format!("> {line}");
        let mut toks = line.split_whitespace();
        let verb = toks.next().unwrap_or("").to_uppercase();
        let rest: Vec<&str> = toks.collect();
        match verb.as_str() {
            "G" | "GET" | "OPEN" => {
                if rest.is_empty() {
                    self.open_dialog();
                } else {
                    let path = self.resolve_path(&rest.join(" "));
                    self.open_path(path);
                }
            }
            "SA" => {
                if rest.is_empty() {
                    self.save_as_dialog();
                } else {
                    let path = self.resolve_path(&rest.join(" "));
                    self.save_to(path);
                }
            }
            "SM" => self.save_same(),
            "LAST" => {
                if let Some(p) = self.recent.first().cloned() {
                    self.open_path(p);
                } else {
                    self.status = "No recent file.".into();
                }
            }
            "I" => {
                let n = rest
                    .first()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                if n >= 1 && n <= self.score.items.len() {
                    self.selected = Some(n - 1);
                    self.status = format!("Item {}", n);
                } else if let Some(last) = self.score.items.len().checked_sub(1) {
                    self.selected = Some(last);
                    self.status = format!("Item {}", last + 1);
                }
            }
            "II" => {
                if let Some(last) = self.score.items.len().checked_sub(1) {
                    self.selected = Some(last);
                    self.status = format!("Item {}", last + 1);
                }
            }
            "IN" | "STA" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or_else(|| self.score.highest_staff() + 1)
                    .clamp(1, 256);
                if !self
                    .score
                    .items
                    .iter()
                    .any(|i| i.code == 8 && i.staff() == staff)
                {
                    self.push_undo();
                    self.score
                        .add_item(Item::new(8, vec![8.0, staff as f32, 0.0, 0.0, 1.0, 200.0]));
                    self.mark_changed();
                }
                self.tool = Tool::Note { head: 0, dur: 1.0 };
                self.status =
                    format!("Input on staff {staff}: choose a note value and click the staff.");
            }
            "NOTE" | "NOTES" | "NOTED" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.score.highest_staff());
                self.tool = Tool::Note { head: 0, dur: 1.0 };
                let (p3, p4) = (50.0, 7.0);
                self.push_undo();
                let idx = self.score.add_item(Item::new(
                    1,
                    vec![1.0, staff as f32, p3, p4, 10.0, 0.0, 1.0, 0.0, 0.0],
                ));
                self.selected = Some(idx);
                self.mark_changed();
                self.status = "Quarter note added.".into();
            }
            "REST" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.score.highest_staff());
                self.push_undo();
                let idx = self.score.add_item(Item::new(
                    2,
                    vec![2.0, staff as f32, 50.0, 0.0, 0.0, 0.0, 1.0],
                ));
                self.selected = Some(idx);
                self.mark_changed();
                self.status = "Quarter rest added.".into();
            }
            "TRE" | "TREBLE" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.score.highest_staff());
                self.push_undo();
                let idx = self
                    .score
                    .add_item(Item::new(3, vec![3.0, staff as f32, 2.0, 0.0, 0.0]));
                self.selected = Some(idx);
                self.mark_changed();
            }
            "BAS" | "BASS" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.score.highest_staff());
                self.push_undo();
                let idx = self
                    .score
                    .add_item(Item::new(3, vec![3.0, staff as f32, 2.0, 0.0, 1.0]));
                self.selected = Some(idx);
                self.mark_changed();
            }
            "BAR" | "BARLINE" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.score.highest_staff());
                let p3 = rest
                    .get(1)
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(50.0);
                self.push_undo();
                let idx = self
                    .score
                    .add_item(Item::new(14, vec![14.0, staff as f32, p3, 1.0, 0.0]));
                self.selected = Some(idx);
                self.mark_changed();
            }
            "LJ" | "J" => {
                self.status = "Justification: imported SCORE pages keep their engraved P3 positions. Use the arrow keys or command-box L/R/U/D for exact manual adjustment.".into();
            }
            "ST" | "STX" => {
                let staff = rest
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                let codes: Vec<i32> = rest.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
                let mut found = None;
                for (i, it) in self.score.items.iter().enumerate() {
                    if (staff == 0
                        || it.staff() == staff
                        || staff > 256 && it.staff() >= staff - 1000)
                        && (codes.is_empty() || codes.contains(&it.code))
                    {
                        found = Some(i);
                        break;
                    }
                }
                if let Some(i) = found {
                    self.selected = Some(i);
                    self.status = format!("Editing item {}.", i + 1);
                } else {
                    self.status = "No matching item on that staff.".into();
                }
            }
            "X" | "ESC" => {
                self.selected = None;
                self.tool = Tool::Select;
                self.status = "Edit mode ended.".into();
            }
            "DE" | "DEL" => self.delete_selected(),
            "UNDO" => self.do_undo(),
            "REDO" => self.do_redo(),
            "UND" => {
                if let Some(item) = self.copied.clone() {
                    self.push_undo();
                    self.score.items.push(item);
                    self.selected = Some(self.score.items.len() - 1);
                    self.mark_changed();
                    self.status = "Undeleted.".into();
                }
            }
            "Z" | "ZOOM" => {
                self.zoom = rest
                    .first()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1.0f32)
                    .clamp(0.25, 4.0);
                self.texture = None;
                self.status = format!("Zoom {:.0}%.", self.zoom * 100.0);
            }
            "TOP" => self.pending_scroll = Some(0.0),
            "MID" => self.pending_scroll = Some(0.5),
            "BOT" => self.pending_scroll = Some(1.0),
            "L" | "R" | "U" | "D" => {
                let n = rest
                    .first()
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1.0);
                let d = n.max(0.01);
                match verb.as_str() {
                    "L" => self.move_selected(-d, 0.0),
                    "R" => self.move_selected(d, 0.0),
                    "U" => self.move_selected(0.0, d),
                    _ => self.move_selected(0.0, -d),
                }
            }
            "OS" | "OH" | "OT" => {
                self.push_undo();
                self.score.reorder_horizontal();
                self.mark_changed();
                self.status = "Items reordered by staff and horizontal position.".into();
            }
            "PM" => {
                if let Some(idx) = self.selected {
                    let item = &self.score.items[idx];
                    let vals: Vec<String> = item
                        .p
                        .iter()
                        .enumerate()
                        .map(|(i, v)| format!("P{}={:.3}", i + 1, v))
                        .collect();
                    self.status = vals.join("  ");
                }
            }
            "VER" => {
                self.status = format!("WinScore {} — native Rust build", env!("CARGO_PKG_VERSION"))
            }
            "CW" => {
                if let Some(idx) = self.selected {
                    self.push_undo();
                    let item = &mut self.score.items[idx];
                    if item.code == 2 {
                        item.set(9, -1.0);
                    }
                    self.mark_changed();
                }
            }
            "CT" => {
                if let Some(idx) = self.selected {
                    self.push_undo();
                    self.score.items[idx].set(3, 100.0);
                    self.mark_changed();
                }
            }
            _ => {
                // Parameter pairs: 3 40 4 7.5
                let nums: Vec<f32> = rest
                    .iter()
                    .chain(std::iter::once(&verb.as_str()))
                    .filter_map(|s| s.parse::<f32>().ok())
                    .collect();
                // The verb may itself be numeric; rebuild the full argument list.
                let all: Vec<&str> = line.split_whitespace().collect();
                if all.len() >= 2
                    && all.iter().all(|s| s.parse::<f32>().is_ok())
                    && all.len() % 2 == 0
                {
                    if let Some(idx) = self.selected {
                        self.push_undo();
                        let mut pairs = Vec::new();
                        for pair in all.chunks(2) {
                            pairs.push((
                                pair[0].parse::<usize>().unwrap_or(0),
                                pair[1].parse::<f32>().unwrap_or(0.0),
                            ));
                        }
                        let item = &mut self.score.items[idx];
                        for (p, v) in pairs {
                            item.set(p, v);
                        }
                        self.mark_changed();
                        self.status = "Parameters changed.".into();
                    } else {
                        self.status = "Select an item before changing parameters.".into();
                    }
                    let _ = nums;
                } else {
                    self.status = format!("Unknown command: {line}");
                }
            }
        }
    }

    fn apply_fit(&mut self, available: Vec2) {
        if let Some(mode) = self.fit_request.take() {
            let z = match mode {
                FitMode::Page => ((available.x - 42.0) / self.page.width)
                    .min((available.y - 42.0) / self.page.height),
                FitMode::Width => (available.x - 42.0) / self.page.width,
            };
            self.zoom = z.clamp(0.25, 3.0);
            self.texture = None;
        }
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        let scale = (self.zoom * 2.0).clamp(1.0, 3.0);
        let (w, h, rgba) =
            raster::render_rgba(&self.page.draws, self.page.width, self.page.height, scale);
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        self.texture = Some(ctx.load_texture("winscore-page", image, egui::TextureOptions::LINEAR));
        self.tex_scale = scale;
    }

    fn draw_canvas(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        self.last_canvas = avail;
        self.apply_fit(avail);
        let page_size = Vec2::new(self.page.width * self.zoom, self.page.height * self.zoom);
        let canvas_size = Vec2::new(
            page_size.x.max(avail.x).max(100.0) + 44.0,
            page_size.y.max(avail.y).max(100.0) + 44.0,
        );
        let scroll = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
        scroll.show(ui, |ui| {
            let (canvas_rect, response) =
                ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
            let page_rect = Rect::from_center_size(canvas_rect.center(), page_size);
            let painter = ui.painter();
            painter.rect_filled(
                page_rect.translate(Vec2::new(4.0, 5.0)),
                1.0,
                Color32::from_black_alpha(38),
            );
            painter.rect_filled(page_rect, 0.0, Color32::WHITE);
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    page_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painter.rect_stroke(
                page_rect,
                0.0,
                Stroke::new(0.7_f32, Color32::from_gray(90)),
                egui::StrokeKind::Outside,
            );
            // Editing guides: staff numbers are visible on screen but are
            // deliberately absent from the engraved page/PDF/SVG export.
            for st in self.page.staves.values() {
                let pos = Pos2::new(
                    page_rect.min.x + (st.x0 - 15.0) * self.zoom,
                    page_rect.min.y + (st.bottom + 3.0) * self.zoom,
                );
                if page_rect.contains(pos) {
                    painter.text(
                        pos,
                        Align2::RIGHT_TOP,
                        format!("{}", st.id),
                        FontId::monospace(10.0 * self.zoom.max(0.8)),
                        Color32::from_gray(95),
                    );
                }
            }
            if let Some(scroll_frac) = self.pending_scroll.take() {
                let target = Rect::from_min_size(
                    page_rect.min + Vec2::new(0.0, page_size.y * scroll_frac.clamp(0.0, 1.0)),
                    Vec2::new(page_size.x, 1.0),
                );
                ui.scroll_to_rect(target, Some(Align::Center));
            }

            let zoom = self.zoom;
            let to_page = |p: Pos2| -> Pos2 {
                Pos2::new(
                    (p.x - page_rect.min.x) / zoom,
                    (p.y - page_rect.min.y) / zoom,
                )
            };

            if let Some(pos) = response.hover_pos() {
                if page_rect.contains(pos) {
                    let pp = to_page(pos);
                    self.hovered = self.hit_at(pp).map(|h| h.item);
                    if self.hit_at(pp).is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                } else {
                    self.hovered = None;
                }
            }

            // Selection and hover overlays.
            let paint_box = |item: usize, color: Color32, width: f32| {
                for h in self.page.hits.iter().filter(|h| h.item == item) {
                    let r = Rect::from_min_max(
                        Pos2::new(
                            page_rect.min.x + h.min.x * self.zoom,
                            page_rect.min.y + h.min.y * self.zoom,
                        ),
                        Pos2::new(
                            page_rect.min.x + h.max.x * self.zoom,
                            page_rect.min.y + h.max.y * self.zoom,
                        ),
                    );
                    ui.painter().rect_stroke(
                        r.expand(2.0),
                        0.0,
                        Stroke::new(width, color),
                        egui::StrokeKind::Outside,
                    );
                    for c in [
                        r.left_top(),
                        r.right_top(),
                        r.left_bottom(),
                        r.right_bottom(),
                    ] {
                        ui.painter().rect_filled(
                            Rect::from_center_size(c, Vec2::splat(3.0)),
                            0.0,
                            color,
                        );
                    }
                }
            };
            if let Some(h) = self.hovered {
                paint_box(h, Color32::from_rgb(40, 100, 190), 1.0);
            }
            if let Some(s) = self.selected {
                paint_box(s, Color32::from_rgb(205, 32, 32), 1.6);
            }

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let pp = to_page(pos);
                    if page_rect.contains(pos) {
                        if self.tool != Tool::Select {
                            self.add_tool_at(self.tool, pp);
                        } else if self.select_at(pp).is_none() {
                            self.selected = None;
                            self.status =
                                "For Help, press F1.  Choose a tool or click an item.".into();
                        }
                    }
                }
            }
            if response.double_clicked() {
                if let Some(idx) = self.selected {
                    if self.score.items[idx].code == 16 {
                        self.open_text_editor(idx);
                    }
                }
            }
            if response.secondary_clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let pp = to_page(pos);
                    if self.select_at(pp).is_some() {
                        response.clone().context_menu(|ui| {
                            if ui.button("Delete item").clicked() {
                                self.delete_selected();
                                ui.close_menu();
                            }
                            if ui.button("Copy item").clicked() {
                                if let Some(s) = self.selected {
                                    self.copied = self.score.items.get(s).cloned();
                                }
                                ui.close_menu();
                            }
                            if ui.button("Deselect").clicked() {
                                self.selected = None;
                                ui.close_menu();
                            }
                        });
                    }
                }
            }

            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let pp = to_page(pos);
                    if self.select_at(pp).is_some() {
                        if let Some(idx) = self.selected {
                            let it = &self.score.items[idx];
                            self.drag = Some(DragState {
                                item: idx,
                                origin: pp,
                                p3: it.p3(),
                                p4: it.p4(),
                                staff: it.staff(),
                                snapshot: self.score.clone(),
                            });
                        }
                    }
                }
            }
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let pp = to_page(pos);
                    if let Some(drag) = self.drag.clone() {
                        let dx = (pp.x - drag.origin.x) / SCORE_UNIT;
                        let staff_space = self
                            .page
                            .staves
                            .get(&drag.staff)
                            .map(|s| s.space)
                            .unwrap_or(6.3);
                        let dy = -(pp.y - drag.origin.y) / (staff_space * 0.5);
                        let p3 = ((drag.p3 + dx) * 20.0).round() / 20.0;
                        let p4 = ((drag.p4 + dy) * 2.0).round() / 2.0;
                        if let Some(it) = self.score.items.get_mut(drag.item) {
                            it.set(3, p3.clamp(-20.0, 220.0));
                            it.set(4, p4.clamp(-30.0, 40.0));
                        }
                        self.mark_changed();
                        self.status = format!(
                            "Dragging item {}: P3 {:.2}, P4 {:.2}",
                            drag.item + 1,
                            p3,
                            p4
                        );
                    }
                }
            }
            if response.drag_stopped() {
                if let Some(drag) = self.drag.take() {
                    self.undo.push(drag.snapshot);
                    if self.undo.len() > 120 {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                    self.dirty = true;
                }
            }
        });
    }

    fn open_text_editor(&mut self, idx: usize) {
        if let Some(item) = self.score.items.get(idx) {
            if let Some(t) = &item.text {
                self.text_buf = t.display();
                self.text_font_prefix = t
                    .visible_bytes()
                    .iter()
                    .take(3)
                    .map(|b| *b as char)
                    .collect();
                if !self.text_font_prefix.starts_with('_') {
                    self.text_font_prefix = "_00".into();
                }
                self.text_size = 10.0 * self.staff_size(item.staff()) * text_size_factor(item);
                self.text_target = Some(TextTarget::Existing(idx));
                self.show_text_editor = true;
            }
        }
    }

    fn staff_size(&self, staff: i32) -> f32 {
        self.score
            .items
            .iter()
            .find(|i| i.code == 8 && i.staff() == staff)
            .map(|i| {
                if i.get(5).abs() < 0.001 {
                    1.0
                } else {
                    i.get(5).abs()
                }
            })
            .unwrap_or(1.0)
    }

    fn apply_text_editor(&mut self) {
        let prefix = if self.text_font_prefix.starts_with('_') && self.text_font_prefix.len() == 3 {
            self.text_font_prefix.clone()
        } else {
            "_00".to_string()
        };
        let mut raw = prefix.into_bytes();
        raw.extend_from_slice(&crate::model::encode_text(&self.text_buf));
        let text = TextBlock {
            declared_len: raw.len(),
            raw,
        };
        let text_len = text.declared_len as f32;
        match self.text_target.clone() {
            Some(TextTarget::Existing(idx)) => {
                self.push_undo();
                let staff_size = self
                    .score
                    .items
                    .get(idx)
                    .map(|it| self.staff_size(it.staff()))
                    .unwrap_or(1.0);
                if let Some(item) = self.score.items.get_mut(idx) {
                    item.text = Some(text);
                    item.set(12, text_len);
                    item.set(13, 0.0);
                    item.set(6, self.text_size / (10.0 * staff_size).max(0.01));
                    item.set(7, 0.0);
                }
                self.mark_changed();
                self.status = "Text changed.".into();
            }
            Some(TextTarget::New { staff, p3, p4 }) => {
                let mut item = Item::new(
                    16,
                    vec![
                        16.0,
                        staff as f32,
                        p3,
                        p4,
                        0.0,
                        self.text_size / (10.0 * self.staff_size(staff)),
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                    ],
                );
                item.set(12, text_len);
                item.set(13, 0.0);
                item.text = Some(text);
                self.push_undo();
                let idx = self.score.add_item(item);
                self.selected = Some(idx);
                self.mark_changed();
                self.status = "Text added.".into();
            }
            None => {}
        }
        self.show_text_editor = false;
        self.text_target = None;
    }

    fn open_staff_editor(&mut self) {
        self.staff_form.staff = (self.score.highest_staff() + 1).clamp(1, 256);
        self.show_staff_editor = true;
    }

    fn staff_editor_window(&mut self, ctx: &egui::Context) {
        if !self.show_staff_editor {
            return;
        }
        let mut open = self.show_staff_editor;
        let mut create = false;
        egui::Window::new("Create Code 8 Staff")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(390.0)
            .show(ctx, |ui| {
                egui::Grid::new("staff-form")
                    .num_columns(4)
                    .spacing(Vec2::new(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.label("Staff");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.staff)
                                .speed(0.1)
                                .range(1..=256),
                        );
                        ui.label("Size P5");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.p5)
                                .speed(0.01)
                                .range(0.2..=2.0),
                        );
                        ui.end_row();

                        ui.label("Left P3");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.p3)
                                .speed(0.1)
                                .range(0.0..=200.0),
                        );
                        ui.label("Right P6");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.p6)
                                .speed(0.1)
                                .range(0.0..=200.0),
                        );
                        ui.end_row();

                        ui.label("Vertical P4");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.p4)
                                .speed(0.1)
                                .range(-100.0..=100.0),
                        );
                        ui.label("Lines P7");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.lines)
                                .speed(0.1)
                                .range(0..=200),
                        );
                        ui.end_row();

                        ui.label("Instrument P9");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.instrument)
                                .speed(0.1)
                                .range(0..=255),
                        );
                        ui.label("Abs. in P10");
                        ui.add(
                            egui::DragValue::new(&mut self.staff_form.p10)
                                .speed(0.01)
                                .range(-100.0..=100.0),
                        );
                        ui.end_row();
                    });
                ui.label(
                    RichText::new(
                        "P3/P6 are horizontal SCORE units (0–200). P5=1 is a five-line staff; P10 non-zero overrides P4 with inches above staff 1.",
                    )
                    .small(),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create staff").clicked() {
                        create = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_staff_editor = false;
                    }
                });
            });
        if create {
            let f = self.staff_form.clone();
            if self
                .score
                .items
                .iter()
                .any(|i| i.code == 8 && i.staff() == f.staff)
            {
                self.status = format!("Staff {} already exists.", f.staff);
                self.show_staff_editor = true;
            } else {
                self.push_undo();
                let idx = self.score.add_item(Item::new(
                    8,
                    vec![
                        8.0,
                        f.staff as f32,
                        f.p3,
                        f.p4,
                        f.p5,
                        f.p6,
                        f.lines as f32,
                        0.0,
                        f.instrument as f32,
                        f.p10,
                    ],
                ));
                self.selected = Some(idx);
                self.show_staff_editor = false;
                self.mark_changed();
                self.status = format!("Created Code 8 staff {}.", f.staff);
                open = false;
            }
        }
        self.show_staff_editor &= open;
    }

    fn text_editor_window(&mut self, ctx: &egui::Context) {
        if !self.show_text_editor {
            return;
        }
        let mut open = self.show_text_editor;
        egui::Window::new("Edit Text — Code 16")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label("Text (WinScore font escape, e.g. _00 or _02):");
                ui.add(egui::TextEdit::multiline(&mut self.text_buf).desired_rows(4).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Font:");
                    ui.add(egui::TextEdit::singleline(&mut self.text_font_prefix).desired_width(42.0));
                    ui.label("Size:");
                    ui.add(egui::DragValue::new(&mut self.text_size).speed(0.5).range(1.0..=400.0));
                    ui.label("pt");
                });
                ui.label(RichText::new("Tip: WinScore uses _00 Times, _01 Times Bold, _02 Times Italic; << gives é, >> gives è.").small());
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        self.apply_text_editor();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_text_editor = false;
                        self.text_target = None;
                    }
                });
            });
        if !open {
            self.show_text_editor = false;
            self.text_target = None;
        }
    }

    fn show_menu(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New page").clicked() {
                    self.new_score();
                    ui.close_menu();
                }
                if ui.button("Open…   Ctrl+O").clicked() {
                    self.open_dialog();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save   Ctrl+S").clicked() {
                    self.save_same();
                    ui.close_menu();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as_dialog();
                    ui.close_menu();
                }
                ui.separator();
                ui.menu_button("Export", |ui| {
                    if ui.button("PDF (vector)…").clicked() {
                        self.export("pdf");
                        ui.close_menu();
                    }
                    if ui.button("SVG (vector)…").clicked() {
                        self.export("svg");
                        ui.close_menu();
                    }
                    if ui.button("PNG (300 dpi)…").clicked() {
                        self.export("png");
                        ui.close_menu();
                    }
                });
                if ui.button("Print / PDF…").clicked() {
                    self.export("pdf");
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .checkbox(&mut self.show_palette, "Items palette")
                    .changed()
                {
                    ui.close_menu();
                }
                if ui.button("Fit page").clicked() {
                    self.fit_request = Some(FitMode::Page);
                    ui.close_menu();
                }
                if ui.button("Fit width").clicked() {
                    self.fit_request = Some(FitMode::Width);
                    ui.close_menu();
                }
                if ui.button("Zoom 100%").clicked() {
                    self.zoom = 1.0;
                    self.texture = None;
                    ui.close_menu();
                }
                if ui.button("Zoom 200%").clicked() {
                    self.zoom = 2.0;
                    self.texture = None;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Zoom in").clicked() {
                    self.zoom = (self.zoom * 1.12).min(4.0);
                    self.texture = None;
                }
                if ui.button("Zoom out").clicked() {
                    self.zoom = (self.zoom / 1.12).max(0.22);
                    self.texture = None;
                }
            });
            ui.menu_button("Format", |ui| {
                if ui.button("Edit selected text…").clicked() {
                    if let Some(idx) = self.selected {
                        if self.score.items[idx].code == 16 {
                            self.open_text_editor(idx);
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("Center selected text (P3 = 100)").clicked() {
                    if let Some(idx) = self.selected {
                        self.push_undo();
                        self.score.items[idx].set(3, 100.0);
                        self.mark_changed();
                    }
                    ui.close_menu();
                }
                if ui.button("Delete selected item").clicked() {
                    self.delete_selected();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Create Code 8 Staff…").clicked() {
                    self.open_staff_editor();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Reorder by staff (OS)").clicked() {
                    self.execute_command("OS");
                    ui.close_menu();
                }
                if ui.button("Reorder horizontally (OH)").clicked() {
                    self.execute_command("OH");
                    ui.close_menu();
                }
            });
            ui.menu_button("Recent Files", |ui| {
                if self.recent.is_empty() {
                    ui.label("(none yet)");
                }
                let recent = self.recent.clone();
                for p in recent {
                    if ui
                        .button(p.file_name().unwrap_or_default().to_string_lossy())
                        .clicked()
                    {
                        self.open_path(p);
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("SCORE Files in this Directory", |ui| {
                let mut dirs = Vec::new();
                if let Some(parent) = self.score.path.as_ref().and_then(|p| p.parent()) {
                    dirs.push(parent.to_path_buf());
                }
                dirs.push(self.last_dir.clone());
                dirs.push(PathBuf::from("samples"));
                let mut seen = std::collections::BTreeSet::new();
                for dir in dirs {
                    if let Ok(rd) = std::fs::read_dir(&dir) {
                        for e in rd.flatten() {
                            let p = e.path();
                            if p.extension()
                                .and_then(|s| s.to_str())
                                .map(|s| s.eq_ignore_ascii_case("mus"))
                                .unwrap_or(false)
                            {
                                seen.insert(p);
                            }
                        }
                    }
                }
                if seen.is_empty() {
                    ui.label("(no .mus files found)");
                }
                for p in seen {
                    if ui
                        .button(p.file_name().unwrap_or_default().to_string_lossy())
                        .clicked()
                    {
                        self.open_path(p);
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("WinScore Help   F1").clicked() {
                    self.show_help = true;
                    ui.close_menu();
                }
                if ui.button("About WinScore Rust Edition").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("RUST EDITION")
                        .small()
                        .color(Color32::from_gray(80)),
                );
                if let Some(p) = &self.score.path {
                    ui.label(
                        RichText::new(p.file_name().unwrap_or_default().to_string_lossy()).small(),
                    );
                }
            });
        });
    }

    fn show_parameters(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.selected else {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Parameter boxes").strong());
                ui.label(
                    RichText::new("Select an item to see P1–P20.")
                        .small()
                        .italics(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("WINSCORE 5.01")
                            .monospace()
                            .size(10.0)
                            .color(Color32::from_gray(100)),
                    );
                });
            });
            for row in 0..2 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for col in 0..10 {
                        let n = row * 10 + col + 1;
                        ui.label(
                            RichText::new(format!("P{n}"))
                                .small()
                                .monospace()
                                .color(Color32::from_gray(65)),
                        );
                        let (r, _) = ui.allocate_exact_size(Vec2::new(64.0, 16.0), Sense::hover());
                        ui.painter()
                            .rect_filled(r, 0.0, Color32::from_rgb(226, 223, 214));
                        ui.painter().rect_stroke(
                            r,
                            0.0,
                            Stroke::new(1.0_f32, Color32::from_gray(150)),
                            egui::StrokeKind::Inside,
                        );
                    }
                });
            }
            return;
        };
        if idx >= self.score.items.len() {
            self.selected = None;
            return;
        }
        let item_label = self.score.items[idx].label();
        let mut values = [0.0f32; 20];
        for n in 0..20 {
            values[n] = self.score.items[idx].get(n + 1);
        }
        let mut changed = Vec::new();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{}. {}", idx + 1, item_label))
                    .strong()
                    .size(11.0),
            );
            ui.separator();
            let item_preview = self.score.items[idx]
                .text
                .as_ref()
                .map(|t| t.display().chars().take(38).collect::<String>());
            let item_staff = self.score.items[idx].staff();
            let item_is_text = self.score.items[idx].code == 16;
            if let Some(preview) = item_preview {
                ui.label(RichText::new(preview).italics().size(11.0));
            }
            let mut edit_text = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if item_is_text && ui.small_button("Edit Text").clicked() {
                    edit_text = true;
                }
                ui.label(RichText::new(format!("P2 staff {}", item_staff)).small());
            });
            if edit_text {
                self.open_text_editor(idx);
            }
        });
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for col in 0..10 {
                    let n = row * 10 + col + 1;
                    let mut v = values[n - 1];
                    ui.label(
                        RichText::new(format!("P{n}"))
                            .small()
                            .monospace()
                            .color(Color32::from_gray(65)),
                    );
                    let resp = ui.add_sized(
                        [64.0, 18.0],
                        egui::DragValue::new(&mut v)
                            .speed(if n <= 2 { 1.0 } else { 0.05 })
                            .max_decimals(3),
                    );
                    if resp.changed() {
                        changed.push((n, v));
                    }
                }
            });
        }
        if !changed.is_empty() {
            self.push_undo();
            for (n, v) in changed {
                self.score.items[idx].set(n, v);
            }
            self.score.items[idx].code = self.score.items[idx].get(1).round() as i32;
            self.mark_changed();
            self.status = "Parameters changed.".into();
        }
    }

    fn show_status_and_command(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 16.0) * 0.56;
            egui::Frame::default()
                .fill(Color32::from_rgb(196, 193, 186))
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(120)))
                .inner_margin(egui::Margin::symmetric(5, 3))
                .show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    ui.label(
                        RichText::new(&self.status)
                            .size(11.0)
                            .color(Color32::from_gray(30)),
                    );
                });
            ui.add_space(2.0);
            let hint = if self.tool == Tool::Select {
                "command"
            } else {
                "command / click staff to place"
            };
            ui.add_sized(
                [ui.available_width(), 24.0],
                egui::TextEdit::singleline(&mut self.command)
                    .hint_text(hint)
                    .font(egui::TextStyle::Monospace),
            );
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.command.is_empty() {
                let cmd = self.command.clone();
                self.execute_command(&cmd);
            }
        });
    }

    fn tool_button(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        color: Color32,
        tool: Tool,
        tooltip: &str,
    ) {
        let selected = self.tool == tool;
        let mut button =
            egui::Button::new(RichText::new(label).size(9.5).color(Color32::from_gray(15)))
                .fill(color)
                .stroke(Stroke::new(
                    if selected { 2.0_f32 } else { 1.0_f32 },
                    if selected {
                        Color32::from_rgb(190, 30, 30)
                    } else {
                        Color32::from_gray(95)
                    },
                ))
                .corner_radius(2.0)
                .min_size(Vec2::new(30.0, 22.0));
        if !selected {
            button = button.stroke(Stroke::new(1.0_f32, Color32::from_gray(95)));
        }
        let resp = ui.add(button);
        if resp.clicked() {
            self.tool = tool;
            self.status = tooltip.to_string();
        }
        resp.on_hover_text(tooltip);
    }

    fn show_palette(&mut self, ui: &mut egui::Ui) {
        if !self.show_palette {
            return;
        }
        let c_note = Color32::from_rgb(84, 200, 84);
        let c_rest = Color32::from_rgb(236, 222, 40);
        let c_clef = Color32::from_rgb(70, 196, 220);
        let c_key = Color32::from_rgb(242, 160, 72);
        let c_line = Color32::from_rgb(232, 100, 165);
        let c_text = Color32::from_rgb(175, 135, 225);
        let c_bar = Color32::from_rgb(190, 190, 185);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
            ui.label(RichText::new("NOTES").size(9.0).strong());
            self.tool_button(
                ui,
                "W",
                c_note,
                Tool::Note { head: 2, dur: 4.0 },
                "Whole note",
            );
            self.tool_button(
                ui,
                "H",
                c_note,
                Tool::Note { head: 1, dur: 2.0 },
                "Half note",
            );
            self.tool_button(
                ui,
                "Q",
                c_note,
                Tool::Note { head: 0, dur: 1.0 },
                "Quarter note",
            );
            self.tool_button(
                ui,
                "8",
                c_note,
                Tool::Note { head: 0, dur: 0.5 },
                "Eighth note",
            );
            self.tool_button(
                ui,
                "16",
                c_note,
                Tool::Note { head: 0, dur: 0.25 },
                "Sixteenth note",
            );
            self.tool_button(
                ui,
                "32",
                c_note,
                Tool::Note {
                    head: 0,
                    dur: 0.125,
                },
                "Thirty-second note",
            );
            ui.separator();
            ui.label(RichText::new("RESTS").size(9.0).strong());
            self.tool_button(
                ui,
                "W",
                c_rest,
                Tool::Rest { p5: -2, dur: 4.0 },
                "Whole / whole-measure rest",
            );
            self.tool_button(
                ui,
                "H",
                c_rest,
                Tool::Rest { p5: -1, dur: 2.0 },
                "Half rest",
            );
            self.tool_button(
                ui,
                "Q",
                c_rest,
                Tool::Rest { p5: 0, dur: 1.0 },
                "Quarter rest",
            );
            self.tool_button(
                ui,
                "8",
                c_rest,
                Tool::Rest { p5: 1, dur: 0.5 },
                "Eighth rest",
            );
            self.tool_button(
                ui,
                "16",
                c_rest,
                Tool::Rest { p5: 2, dur: 0.25 },
                "Sixteenth rest",
            );
            ui.separator();
            ui.label(RichText::new("ACC").size(9.0).strong());
            for (label, acc) in [("b", 1), ("#", 2), ("n", 3)] {
                let selected = self.accidental == acc;
                let b = egui::Button::new(
                    RichText::new(label)
                        .size(12.0)
                        .color(Color32::from_gray(15)),
                )
                .fill(if selected {
                    Color32::from_rgb(255, 130, 100)
                } else {
                    c_note
                })
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(90)))
                .corner_radius(2.0)
                .min_size(Vec2::new(24.0, 20.0));
                if ui
                    .add(b)
                    .on_hover_text("Accidental for newly placed notes")
                    .clicked()
                {
                    self.accidental = if self.accidental == acc { 0 } else { acc };
                }
            }
            let dot_label = if self.dotted { "DOT ✓" } else { "DOT" };
            if ui
                .add(
                    egui::Button::new(RichText::new(dot_label).size(9.0))
                        .fill(if self.dotted {
                            Color32::from_rgb(255, 150, 100)
                        } else {
                            c_rest
                        })
                        .corner_radius(2.0)
                        .min_size(Vec2::new(34.0, 20.0)),
                )
                .clicked()
            {
                self.dotted = !self.dotted;
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
            ui.label(RichText::new("CLEFS").size(9.0).strong());
            self.tool_button(ui, "TREBLE", c_clef, Tool::Clef(0), "Treble clef");
            self.tool_button(ui, "BASS", c_clef, Tool::Clef(1), "Bass clef");
            self.tool_button(ui, "ALTO", c_clef, Tool::Clef(2), "Alto clef");
            self.tool_button(ui, "TENOR", c_clef, Tool::Clef(3), "Tenor clef");
            ui.separator();
            ui.label(RichText::new("METER").size(9.0).strong());
            self.tool_button(ui, "C", c_key, Tool::Meter(99, 1), "Common time");
            self.tool_button(ui, "3/4", c_key, Tool::Meter(3, 4), "3/4 meter");
            self.tool_button(ui, "4/4", c_key, Tool::Meter(4, 4), "4/4 meter");
            self.tool_button(ui, "6/8", c_key, Tool::Meter(6, 8), "6/8 meter");
            self.tool_button(ui, "2/4", c_key, Tool::Meter(2, 4), "2/4 meter");
            self.tool_button(ui, "SHARP", c_key, Tool::Key(1), "One sharp");
            self.tool_button(ui, "FLAT", c_key, Tool::Key(-1), "One flat");
            ui.separator();
            ui.label(RichText::new("BARS").size(9.0).strong());
            self.tool_button(ui, "BAR", c_bar, Tool::Barline(0), "Barline");
            self.tool_button(ui, "DBL", c_bar, Tool::Barline(1), "Double barline");
            self.tool_button(ui, "FINAL", c_bar, Tool::Barline(2), "Final barline");
            self.tool_button(ui, "REP", c_bar, Tool::Barline(4), "Repeat barline");
            self.tool_button(ui, "BRACE", c_bar, Tool::Barline(8), "Brace");
            if ui
                .add(
                    egui::Button::new(RichText::new("STAFF").size(9.0))
                        .fill(c_clef)
                        .corner_radius(2.0)
                        .min_size(Vec2::new(46.0, 20.0)),
                )
                .on_hover_text("Create a Code 8 staff")
                .clicked()
            {
                self.open_staff_editor();
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
            ui.label(RichText::new("LINES").size(9.0).strong());
            self.tool_button(ui, "SLUR", c_line, Tool::Slur, "Slur");
            self.tool_button(ui, "BEAM", c_line, Tool::Beam, "Beam");
            self.tool_button(ui, "CRESC", c_line, Tool::Hairpin, "Hairpin");
            ui.separator();
            ui.label(RichText::new("TEXT / MARKS").size(9.0).strong());
            self.tool_button(ui, "TEXT", c_text, Tool::Text, "Text");
            self.tool_button(ui, "NUM", c_text, Tool::Number, "Number / rehearsal mark");
            self.tool_button(ui, "p", c_text, Tool::Dynamic(53), "piano");
            self.tool_button(ui, "mf", c_text, Tool::Dynamic(55), "mezzo-forte");
            self.tool_button(ui, "f", c_text, Tool::Dynamic(56), "forte");
            self.tool_button(
                ui,
                "cresc.",
                c_text,
                Tool::Dynamic(63),
                "crescendo (symbol)",
            );
            self.tool_button(ui, "rit.", c_text, Tool::Dynamic(65), "ritardando");
            ui.separator();
            ui.label(RichText::new("HELP").size(9.0).strong());
            if ui
                .add(
                    egui::Button::new(RichText::new("HELP (F1)").size(9.0))
                        .fill(Color32::from_rgb(210, 210, 205))
                        .corner_radius(2.0)
                        .min_size(Vec2::new(58.0, 20.0)),
                )
                .clicked()
            {
                self.show_help = true;
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(RichText::new("ITEM BAR OFF").size(9.0))
                        .fill(Color32::from_rgb(210, 210, 205))
                        .corner_radius(2.0)
                        .min_size(Vec2::new(86.0, 20.0)),
                )
                .clicked()
            {
                self.show_palette = false;
                self.tool = Tool::Select;
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
            ui.label(RichText::new("MARKINGS").size(9.0).strong());
            self.tool_button(ui, "tr~", c_line, Tool::Trill, "Trill with extension");
            self.tool_button(ui, "8va", c_line, Tool::Ottava(8), "Ottava above");
            self.tool_button(ui, "15m", c_line, Tool::Ottava(15), "Fifteenth above");
            self.tool_button(ui, "PED", c_line, Tool::Pedal(20), "Pedal mark");
            self.tool_button(ui, "PED~", c_line, Tool::Pedal(21), "Pedal with extension");
            self.tool_button(ui, "1.", c_line, Tool::Ending(1), "First ending bracket");
            self.tool_button(ui, "2.", c_line, Tool::Ending(2), "Second ending bracket");
        });
    }

    fn show_main_toolbar(&mut self, ui: &mut egui::Ui) {
        let font = 9.0;
        let btn = |ui: &mut egui::Ui, label: &str, color: Color32| -> bool {
            ui.add(
                egui::Button::new(
                    RichText::new(label)
                        .size(font)
                        .color(Color32::from_gray(20)),
                )
                .fill(color)
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(95)))
                .corner_radius(2.0)
                .min_size(Vec2::new(48.0, 19.0)),
            )
            .clicked()
        };
        let c1 = Color32::from_rgb(195, 195, 190);
        let c2 = Color32::from_rgb(82, 205, 205);
        let c3 = Color32::from_rgb(105, 220, 105);
        let c4 = Color32::from_rgb(240, 225, 60);
        let c5 = Color32::from_rgb(245, 155, 70);
        let c6 = Color32::from_rgb(230, 105, 170);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
            if btn(ui, "FULL PAGE", c1) { self.fit_request = Some(FitMode::Page); }
            if btn(ui, "FIT WIDTH", c1) { self.fit_request = Some(FitMode::Width); }
            if btn(ui, "ZOOM −", c2) { self.zoom = (self.zoom / 1.12).max(0.22); self.texture = None; }
            if btn(ui, "ZOOM +", c2) { self.zoom = (self.zoom * 1.12).min(4.0); self.texture = None; }
            if btn(ui, "PLAY", c3) {
                match crate::playback::play(&self.score) {
                    Ok(msg) => self.status = msg,
                    Err(err) => self.status = format!("Playback unavailable: {err}"),
                }
            }
            if btn(ui, "NEW ITEM", c3) {
                self.show_palette = true;
                self.status = "Choose an item from the item bar.".into();
            }
            if btn(ui, "SAVE SAME", c4) { self.save_same(); }
            if btn(ui, "OS", c4) { self.execute_command("OS"); }
            if btn(ui, "OH", c4) { self.execute_command("OH"); }
            if btn(ui, "OT", c4) { self.execute_command("OT"); }
            if btn(ui, "RS", c2) { self.new_score(); }
            if btn(ui, "RSM", c2) { if let Some(p) = self.score.path.clone() { self.open_path(p); } }
            if btn(ui, "GP", c2) { if let Some(p) = self.recent.get(1).cloned() { self.open_path(p); } }
            if btn(ui, "LAST ITEM", c1) { if let Some(i) = self.score.items.len().checked_sub(1) { self.selected = Some(i); } }
            if btn(ui, "PRINT", c5) { self.export("pdf"); }
            if btn(ui, "H", c4) { self.status = format!("Page height {:.1} pt.", self.page.height); }
            if btn(ui, "HS", c4) { self.status = "Staff height 0.35 in at size 1.0 (P5).".into(); }
            if btn(ui, "Z", c4) { self.zoom = if (self.zoom - 2.0).abs() < 0.01 { 1.0 } else { 2.0 }; self.texture = None; }
            if btn(ui, "PAPER SIZE", c5) { self.status = "WinScore page, 554 × 643 pt; music area 540 pt (7.5 in) wide.".into(); }
            if btn(ui, "DRAW", c6) { self.status = "DRAW: use the symbol palette and Code 9 objects to design new glyphs.".into(); }
            if btn(ui, "PAGE LAYOUT", c5) { self.status = "Layout: staves follow SCORE P4/P5 positioning; use VJ for vertical justify in a future release.".into(); }
            if btn(ui, "PARTS", c5) { self.status = "Part extraction is not enabled in this build; the original staff/instrument mapping is preserved.".into(); }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
            if btn(ui, "TOP", c1) {
                self.pending_scroll = Some(0.0);
            }
            if btn(ui, "MID", c1) {
                self.pending_scroll = Some(0.5);
            }
            if btn(ui, "BOT", c1) {
                self.pending_scroll = Some(1.0);
            }
            if btn(ui, "FREEZE", c4) {
                self.status = "Freeze display: auto-scroll disabled (placeholder).".into();
            }
            if btn(ui, "X", c6) {
                self.selected = None;
                self.tool = Tool::Select;
            }
            if btn(ui, "PASTE", c2) {
                if let Some(item) = self.copied.clone() {
                    self.push_undo();
                    let idx = self.score.add_item(item);
                    self.selected = Some(idx);
                    self.mark_changed();
                }
            }
            if btn(ui, "GROUP EDIT", c3) {
                self.status = "Group edit: use OS/OH/OT and parameter edits.".into();
            }
            if btn(ui, "INPUT", c3) {
                self.status = "Input mode: choose a note value above and click the staff.".into();
            }
            if btn(ui, "EDIT INPUT", c3) {
                self.status = "EDIT INPUT: reconstructing a text input line is future work.".into();
            }
            if btn(ui, "ED", c3) {
                self.status = "ED: click an item to edit it.".into();
            }
            if btn(ui, "BEAM", c3) {
                self.tool = Tool::Beam;
            }
            if btn(ui, "READ LAST", c4) {
                if let Some(p) = self.recent.first().cloned() {
                    self.open_path(p);
                }
            }
            if btn(ui, "SC", c4) {
                self.status = "Spacing scale: horizontal 0–200 units over 7.5 inches.".into();
            }
            if btn(ui, "SCX", c4) {
                self.status = "Spacing scale off.".into();
            }
            if btn(ui, "UNDO", c6) {
                self.do_undo();
            }
            if btn(ui, "REDO", c6) {
                self.do_redo();
            }
            if btn(ui, "UND", c6) {
                self.execute_command("UND");
            }
            if btn(ui, "DEL", c6) {
                self.delete_selected();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}%", self.zoom * 100.0))
                        .monospace()
                        .size(11.0),
                );
            });
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let mut open = true;
        egui::Window::new("WinScore 5.01 — Help")
            .open(&mut open)
            .default_size(Vec2::new(640.0, 520.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("A native Rust edition of Leland Smith's WinScore");
                    ui.label("The page is drawn from the original SCORE object model: 200 horizontal units over 7.5 inches, and vertical positions measured in diatonic scale steps.");
                    ui.separator();
                    ui.strong("Mouse");
                    ui.label("• Left-click a note or symbol to edit it.\n• Drag a selected object to move it.\n• Right-click for Copy / Delete.\n• Double-click text to edit it.\n• Choose a tool in the Items palette, then click on the staff to place it.");
                    ui.separator();
                    ui.strong("Keyboard");
                    ui.label("Arrows: nudge selected item  |  Delete: delete  |  Ctrl+Z / Ctrl+Shift+Z: undo / redo\nCtrl+S: save  |  Ctrl+O: open  |  Esc: leave edit mode  |  F1: help");
                    ui.separator();
                    ui.strong("Command box (SCORE-style)");
                    ui.label("G file — get/open a file\nSA file — save as\nSM — save same\nI n — edit item n\nST staff codes — find items on a staff\nX — end edit\n3 42.5 — set P3 to 42.5\nL 2, R 2, U 1, D 1 — move\nOS, OH, OT — reorder objects\nUNDO, REDO, UND — history\nTOP, MID, BOT — scroll the page\nZ 1.5 — zoom");
                    ui.separator();
                    ui.strong("File compatibility");
                    ui.label("Reads and writes native binary WinScore .mus files, including code-16 text objects with their original font escapes and word padding. Unknown object codes are preserved even when they are not yet drawn.");
                    ui.label("The parameter model follows the published SCORE documentation: code 1 notes, 2 rests, 3 clefs, 4 lines, 5 slurs, 6 beams, 7 trills/ottavas, 8 staves, 9 symbol library, 10 numbers, 12 boxes, 14 barlines/braces, 16 text, 17 keys, 18 meters.");
                });
            });
        self.show_help = open;
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("About")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("WinScore 5.01 — Rust Edition");
                ui.label("An independent reimplementation of the legendary SCORE/WinScore engraver.");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label("Engraving: native Rust vector layout, Bravura SMuFL outlines, Times New Roman text outlines.");
                ui.label("File format: SCORE/WinScore binary .mus, verified by bit-exact round-trips of the supplied 1997–2013 sample scores.");
                ui.label("Exports: PNG, SVG and vector PDF with outlined glyphs.");
                ui.separator();
                ui.label(RichText::new("Bravura is licensed under the SIL Open Font License. Liberation Serif is licensed under the SIL OFL. Times New Roman is used from the host system when available.").small());
            });
        self.show_about = open;
    }
}

impl eframe::App for WinScoreApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts only when a text field is not focused.
        if !ctx.wants_keyboard_input() {
            let (undo, redo, save, open, delete, esc) = ctx.input(|i| {
                (
                    i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
                    i.modifiers.command
                        && (i.key_pressed(egui::Key::Y)
                            || (i.modifiers.shift && i.key_pressed(egui::Key::Z))),
                    i.modifiers.command && i.key_pressed(egui::Key::S),
                    i.modifiers.command && i.key_pressed(egui::Key::O),
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if undo {
                self.do_undo();
            }
            if redo {
                self.do_redo();
            }
            if save {
                self.save_same();
            }
            if open {
                self.open_dialog();
            }
            if delete && self.selected.is_some() {
                self.delete_selected();
            }
            if esc {
                self.selected = None;
                self.tool = Tool::Select;
            }
            if self.selected.is_some() {
                let mut dx = 0.0;
                let mut dy = 0.0;
                let step = if ctx.input(|i| i.modifiers.shift) {
                    0.5
                } else {
                    0.5
                };
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    dx -= step;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    dx += step;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    dy += step;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    dy -= step;
                }
                if dx != 0.0 || dy != 0.0 {
                    self.move_selected(dx, dy);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
                self.show_help = true;
            }
        }

        let title = format!(
            "{}{} — {}",
            if self.dirty { "● " } else { "" },
            self.score
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "UNTITLED.MUS".into()),
            APP_NAME
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::TopBottomPanel::top("menu")
            .exact_height(24.0)
            .show(ctx, |ui| {
                self.show_menu(ui);
            });
        egui::TopBottomPanel::top("params")
            .exact_height(70.0)
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(214, 211, 202))
                    .inner_margin(egui::Margin::symmetric(4, 2)),
            )
            .show(ctx, |ui| {
                self.show_parameters(ui);
            });
        egui::TopBottomPanel::top("status")
            .exact_height(36.0)
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(224, 221, 212))
                    .inner_margin(egui::Margin::symmetric(4, 1)),
            )
            .show(ctx, |ui| {
                self.show_status_and_command(ui);
            });
        // WinScore's first-level command bar lives at the top.  The item
        // palette is a second-level screen opened by NEW ITEM (or View →
        // Items palette) and replaces the command bar while it is open.
        if self.show_palette {
            egui::TopBottomPanel::top("palette")
                .exact_height(100.0)
                .frame(
                    egui::Frame::default()
                        .fill(Color32::from_rgb(204, 201, 193))
                        .inner_margin(egui::Margin::symmetric(4, 1)),
                )
                .show(ctx, |ui| {
                    self.show_palette(ui);
                });
        } else {
            egui::TopBottomPanel::top("toolbar")
                .exact_height(48.0)
                .frame(
                    egui::Frame::default()
                        .fill(Color32::from_rgb(201, 198, 190))
                        .inner_margin(egui::Margin::symmetric(4, 1)),
                )
                .show(ctx, |ui| {
                    self.show_main_toolbar(ui);
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::from_rgb(154, 152, 146)))
            .show(ctx, |ui| {
                self.ensure_texture(ctx);
                self.draw_canvas(ui);
            });

        self.staff_editor_window(ctx);
        self.text_editor_window(ctx);
        self.help_window(ctx);
        self.about_window(ctx);
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(Color32::from_gray(25));
    visuals.panel_fill = Color32::from_rgb(212, 209, 201);
    visuals.window_fill = Color32::from_rgb(218, 215, 206);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(228, 225, 217);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(205, 202, 194);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(120));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(224, 220, 211);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(80));
    visuals.widgets.active.bg_fill = Color32::from_rgb(235, 231, 220);
    visuals.window_corner_radius = egui::CornerRadius::same(3);
    visuals.menu_corner_radius = egui::CornerRadius::same(2);
    ctx.set_visuals(visuals);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(4.0, 3.0);
    style.spacing.button_padding = Vec2::new(5.0, 2.0);
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(12.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(11.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new(10.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(11.0, egui::FontFamily::Monospace),
    );
    ctx.set_style(style);
}

fn text_size_factor(item: &Item) -> f32 {
    let mut p6 = item.get(6).abs();
    if p6 >= 300.0 {
        p6 -= 300.0;
    } else if p6 >= 200.0 {
        p6 -= 200.0;
    } else if p6 >= 100.0 {
        p6 -= 100.0;
    }
    if p6 < 0.001 { 1.0 } else { p6 }
}
