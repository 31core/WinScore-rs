//! CPU rasterisation of a `DrawList` with tiny-skia.

use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::draw::{Color, DrawList, Path, PathCmd};

pub fn render_rgba(
    draws: &DrawList,
    width_pt: f32,
    height_pt: f32,
    scale: f32,
) -> (u32, u32, Vec<u8>) {
    let w = (width_pt * scale).ceil().max(1.0) as u32;
    let h = (height_pt * scale).ceil().max(1.0) as u32;
    let mut pixmap = Pixmap::new(w, h).expect("valid pixmap dimensions");
    pixmap.fill(tiny_skia::Color::WHITE);

    for paint in &draws.paints {
        let Some(path) = to_skia_path(&paint.path, scale) else {
            continue;
        };
        if let Some(fill) = paint.fill {
            let mut p = Paint::default();
            p.set_color_rgba8(fill.r, fill.g, fill.b, fill.a);
            p.anti_alias = true;
            pixmap.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
        if let Some(stroke) = paint.stroke {
            let mut p = Paint::default();
            p.set_color_rgba8(
                stroke.color.r,
                stroke.color.g,
                stroke.color.b,
                stroke.color.a,
            );
            p.anti_alias = true;
            let s = Stroke {
                width: (stroke.width * scale).max(0.35),
                line_cap: if stroke.round_cap {
                    LineCap::Round
                } else {
                    LineCap::Butt
                },
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &p, &s, Transform::identity(), None);
        }
    }

    (w, h, pixmap.data().to_vec())
}

pub fn render_png(
    draws: &DrawList,
    width_pt: f32,
    height_pt: f32,
    scale: f32,
) -> anyhow::Result<Vec<u8>> {
    let (w, h, data) = render_rgba(draws, width_pt, height_pt, scale);
    let img = image::RgbaImage::from_raw(w, h, data)
        .ok_or_else(|| anyhow::anyhow!("could not create page image"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

fn to_skia_path(path: &Path, scale: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for cmd in &path.cmds {
        match *cmd {
            PathCmd::MoveTo(p) => pb.move_to(p.x * scale, p.y * scale),
            PathCmd::LineTo(p) => pb.line_to(p.x * scale, p.y * scale),
            PathCmd::QuadTo(a, b) => pb.quad_to(a.x * scale, a.y * scale, b.x * scale, b.y * scale),
            PathCmd::CubicTo(a, b, c) => pb.cubic_to(
                a.x * scale,
                a.y * scale,
                b.x * scale,
                b.y * scale,
                c.x * scale,
                c.y * scale,
            ),
            PathCmd::Close => pb.close(),
        }
    }
    pb.finish()
}

/// A small convenience used by the on-screen page shadow.
pub fn rgba_hex(c: Color) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}
