//! Resolution-independent drawing primitives shared by the screen renderer,
//! SVG and PDF exporters.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const INK: Self = Self::rgb(24, 23, 22);
    pub const SOFT_INK: Self = Self::rgb(55, 54, 52);
    pub const GRAY: Self = Self::rgb(120, 120, 118);
    pub const LIGHT_GRAY: Self = Self::rgb(190, 190, 186);
    pub const RED: Self = Self::rgb(196, 36, 36);
    pub const BLUE: Self = Self::rgb(30, 86, 160);
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
    pub fn css(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({},{},{},{:.3})",
                self.r,
                self.g,
                self.b,
                self.a as f32 / 255.0
            )
        }
    }
    pub fn pdf_rgb(&self) -> (f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    MoveTo(Pt),
    LineTo(Pt),
    QuadTo(Pt, Pt),
    CubicTo(Pt, Pt, Pt),
    Close,
}

#[derive(Debug, Clone, Default)]
pub struct Path {
    pub cmds: Vec<PathCmd>,
    pub min: Pt,
    pub max: Pt,
    pub empty: bool,
}

impl Path {
    pub fn new() -> Self {
        Self {
            cmds: Vec::new(),
            min: Pt::new(f32::MAX, f32::MAX),
            max: Pt::new(f32::MIN, f32::MIN),
            empty: true,
        }
    }

    pub fn from_cmds(cmds: Vec<PathCmd>) -> Self {
        let mut p = Self::new();
        for c in cmds {
            p.push(c);
        }
        p
    }

    pub fn push(&mut self, cmd: PathCmd) {
        match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => self.expand(p),
            PathCmd::QuadTo(a, b) => {
                self.expand(a);
                self.expand(b);
            }
            PathCmd::CubicTo(a, b, c) => {
                self.expand(a);
                self.expand(b);
                self.expand(c);
            }
            PathCmd::Close => {}
        }
        self.empty = false;
        self.cmds.push(cmd);
    }

    pub fn expand(&mut self, p: Pt) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn width(&self) -> f32 {
        if self.empty {
            0.0
        } else {
            self.max.x - self.min.x
        }
    }
    pub fn height(&self) -> f32 {
        if self.empty {
            0.0
        } else {
            self.max.y - self.min.y
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty || self.cmds.is_empty()
    }

    pub fn transform(mut self, tx: f32, ty: f32, angle_deg: f32) -> Self {
        if angle_deg.abs() < 0.001 {
            for c in &mut self.cmds {
                transform_cmd(c, tx, ty, 0.0, 0.0);
            }
            self.min.x += tx;
            self.max.x += tx;
            self.min.y += ty;
            self.max.y += ty;
            return self;
        }
        let a = angle_deg.to_radians();
        let (s, co) = a.sin_cos();
        for c in &mut self.cmds {
            let tf = |p: Pt| Pt::new(tx + p.x * co - p.y * s, ty + p.x * s + p.y * co);
            *c = match *c {
                PathCmd::MoveTo(p) => PathCmd::MoveTo(tf(p)),
                PathCmd::LineTo(p) => PathCmd::LineTo(tf(p)),
                PathCmd::QuadTo(a, b) => PathCmd::QuadTo(tf(a), tf(b)),
                PathCmd::CubicTo(a, b, c) => PathCmd::CubicTo(tf(a), tf(b), tf(c)),
                PathCmd::Close => PathCmd::Close,
            };
        }
        // Recompute bbox.
        let mut out = Path::new();
        for c in self.cmds {
            out.push(c);
        }
        out
    }

    pub fn translate(mut self, tx: f32, ty: f32) -> Self {
        for c in &mut self.cmds {
            transform_cmd(c, tx, ty, 0.0, 0.0);
        }
        if !self.empty {
            self.min.x += tx;
            self.max.x += tx;
            self.min.y += ty;
            self.max.y += ty;
        }
        self
    }

    pub fn scale(mut self, sx: f32, sy: f32) -> Self {
        for c in &mut self.cmds {
            match c {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => {
                    p.x *= sx;
                    p.y *= sy;
                }
                PathCmd::QuadTo(a, b) => {
                    a.x *= sx;
                    a.y *= sy;
                    b.x *= sx;
                    b.y *= sy;
                }
                PathCmd::CubicTo(a, b, d) => {
                    a.x *= sx;
                    a.y *= sy;
                    b.x *= sx;
                    b.y *= sy;
                    d.x *= sx;
                    d.y *= sy;
                }
                PathCmd::Close => {}
            }
        }
        if !self.empty {
            self.min.x *= sx;
            self.min.y *= sy;
            self.max.x *= sx;
            self.max.y *= sy;
        }
        self
    }

    pub fn svg_d(&self) -> String {
        let mut s = String::new();
        for c in &self.cmds {
            match c {
                PathCmd::MoveTo(p) => s.push_str(&format!("M{:.3} {:.3} ", p.x, p.y)),
                PathCmd::LineTo(p) => s.push_str(&format!("L{:.3} {:.3} ", p.x, p.y)),
                PathCmd::QuadTo(a, b) => {
                    s.push_str(&format!("Q{:.3} {:.3} {:.3} {:.3} ", a.x, a.y, b.x, b.y))
                }
                PathCmd::CubicTo(a, b, c) => s.push_str(&format!(
                    "C{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} ",
                    a.x, a.y, b.x, b.y, c.x, c.y
                )),
                PathCmd::Close => s.push_str("Z "),
            }
        }
        s
    }
}

fn transform_cmd(c: &mut PathCmd, tx: f32, ty: f32, _ax: f32, _ay: f32) {
    let f = |p: &mut Pt| {
        p.x += tx;
        p.y += ty;
    };
    match c {
        PathCmd::MoveTo(p) | PathCmd::LineTo(p) => f(p),
        PathCmd::QuadTo(a, b) => {
            f(a);
            f(b);
        }
        PathCmd::CubicTo(a, b, d) => {
            f(a);
            f(b);
            f(d);
        }
        PathCmd::Close => {}
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub round_cap: bool,
}

#[derive(Debug, Clone)]
pub struct Paint {
    pub path: Path,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

#[derive(Debug, Clone, Default)]
pub struct DrawList {
    pub paints: Vec<Paint>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, paint: Paint) {
        if !paint.path.is_empty() {
            self.paints.push(paint);
        }
    }

    pub fn fill(&mut self, path: Path, color: Color) {
        self.push(Paint {
            path,
            fill: Some(color),
            stroke: None,
        });
    }

    pub fn stroke(&mut self, path: Path, color: Color, width: f32) {
        self.push(Paint {
            path,
            fill: None,
            stroke: Some(Stroke {
                color,
                width,
                round_cap: true,
            }),
        });
    }

    pub fn line(&mut self, a: Pt, b: Pt, color: Color, width: f32) {
        let mut p = Path::new();
        p.push(PathCmd::MoveTo(a));
        p.push(PathCmd::LineTo(b));
        self.push(Paint {
            path: p,
            fill: None,
            stroke: Some(Stroke {
                color,
                width,
                round_cap: false,
            }),
        });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let mut p = Path::new();
        p.push(PathCmd::MoveTo(Pt::new(x, y)));
        p.push(PathCmd::LineTo(Pt::new(x + w, y)));
        p.push(PathCmd::LineTo(Pt::new(x + w, y + h)));
        p.push(PathCmd::LineTo(Pt::new(x, y + h)));
        p.push(PathCmd::Close);
        self.fill(p, color);
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, width: f32) {
        let mut p = Path::new();
        p.push(PathCmd::MoveTo(Pt::new(x, y)));
        p.push(PathCmd::LineTo(Pt::new(x + w, y)));
        p.push(PathCmd::LineTo(Pt::new(x + w, y + h)));
        p.push(PathCmd::LineTo(Pt::new(x, y + h)));
        p.push(PathCmd::Close);
        self.stroke(p, color, width);
    }

    pub fn dotted_line(&mut self, a: Pt, b: Pt, color: Color, width: f32, dash: f32, gap: f32) {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0 {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let mut t = 0.0;
        while t < len {
            let t2 = (t + dash).min(len);
            let mut p = Path::new();
            p.push(PathCmd::MoveTo(Pt::new(a.x + ux * t, a.y + uy * t)));
            p.push(PathCmd::LineTo(Pt::new(a.x + ux * t2, a.y + uy * t2)));
            self.push(Paint {
                path: p,
                fill: None,
                stroke: Some(Stroke {
                    color,
                    width,
                    round_cap: false,
                }),
            });
            t += dash + gap;
        }
    }

    pub fn dashed_line(&mut self, a: Pt, b: Pt, color: Color, width: f32, dash: f32) {
        self.dotted_line(a, b, color, width, dash, dash * 0.65);
    }

    pub fn append(&mut self, other: DrawList) {
        self.paints.extend(other.paints);
    }
}
