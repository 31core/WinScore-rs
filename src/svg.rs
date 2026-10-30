//! Self-contained SVG export.  Text is already converted to outlines, so the
//! SVG has no font dependencies.

use crate::draw::{DrawList, PathCmd};

pub fn to_svg(draws: &DrawList, width_pt: f32, height_pt: f32) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.2}pt\" height=\"{h:.2}pt\" viewBox=\"0 0 {w:.2} {h:.2}\">\n",
        w = width_pt,
        h = height_pt
    ));
    s.push_str("<title>WinScore page</title>\n");
    s.push_str(&format!(
        "<rect width=\"{:.2}\" height=\"{:.2}\" fill=\"#ffffff\"/>\n",
        width_pt, height_pt
    ));
    for paint in &draws.paints {
        let d = path_d(&paint.path);
        if d.is_empty() {
            continue;
        }
        s.push_str("<path d=\"");
        s.push_str(&d);
        s.push('"');
        if let Some(fill) = paint.fill {
            s.push_str(&format!(" fill=\"{}\"", fill.css()));
        } else {
            s.push_str(" fill=\"none\"");
        }
        if let Some(stroke) = paint.stroke {
            s.push_str(&format!(
                " stroke=\"{}\" stroke-width=\"{:.3}\" stroke-linecap=\"{}\" stroke-linejoin=\"round\"",
                stroke.color.css(),
                stroke.width,
                if stroke.round_cap { "round" } else { "butt" }
            ));
        }
        s.push_str("/>\n");
    }
    s.push_str("</svg>\n");
    s
}

fn path_d(path: &crate::draw::Path) -> String {
    let mut d = String::new();
    for c in &path.cmds {
        match c {
            PathCmd::MoveTo(p) => d.push_str(&format!("M{:.3},{:.3} ", p.x, p.y)),
            PathCmd::LineTo(p) => d.push_str(&format!("L{:.3},{:.3} ", p.x, p.y)),
            PathCmd::QuadTo(a, b) => {
                d.push_str(&format!("Q{:.3},{:.3} {:.3},{:.3} ", a.x, a.y, b.x, b.y))
            }
            PathCmd::CubicTo(a, b, c) => d.push_str(&format!(
                "C{:.3},{:.3} {:.3},{:.3} {:.3},{:.3} ",
                a.x, a.y, b.x, b.y, c.x, c.y
            )),
            PathCmd::Close => d.push_str("Z "),
        }
    }
    d
}
