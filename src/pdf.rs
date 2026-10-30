//! Minimal, dependency-free PDF 1.4 writer for engraved pages.
//!
//! Every glyph is already a vector outline, so the output is true vector art
//! and does not need font embedding.  The coordinate transform maps the
//! engraver's top-left point coordinates into PDF's bottom-left space.

use crate::draw::{DrawList, PathCmd};

pub fn to_pdf(draws: &DrawList, width_pt: f32, height_pt: f32) -> Vec<u8> {
    let mut content = String::new();
    for paint in &draws.paints {
        let mut ops = String::new();
        let mut current = (0.0f32, 0.0f32);
        for c in &paint.path.cmds {
            match c {
                PathCmd::MoveTo(p) => {
                    ops.push_str(&format!("{:.3} {:.3} m\n", p.x, height_pt - p.y));
                    current = (p.x, p.y);
                }
                PathCmd::LineTo(p) => {
                    ops.push_str(&format!("{:.3} {:.3} l\n", p.x, height_pt - p.y));
                    current = (p.x, p.y);
                }
                PathCmd::QuadTo(a, b) => {
                    let c1 = (
                        current.0 + 2.0 / 3.0 * (a.x - current.0),
                        current.1 + 2.0 / 3.0 * (a.y - current.1),
                    );
                    let c2 = (b.x + 2.0 / 3.0 * (a.x - b.x), b.y + 2.0 / 3.0 * (a.y - b.y));
                    ops.push_str(&format!(
                        "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n",
                        c1.0,
                        height_pt - c1.1,
                        c2.0,
                        height_pt - c2.1,
                        b.x,
                        height_pt - b.y
                    ));
                    current = (b.x, b.y);
                }
                PathCmd::CubicTo(a, b, c) => {
                    ops.push_str(&format!(
                        "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c\n",
                        a.x,
                        height_pt - a.y,
                        b.x,
                        height_pt - b.y,
                        c.x,
                        height_pt - c.y
                    ));
                    current = (c.x, c.y);
                }
                PathCmd::Close => ops.push_str("h\n"),
            }
        }
        if ops.is_empty() {
            continue;
        }
        content.push_str("q\n");
        if let Some(fill) = paint.fill {
            let (r, g, b) = fill.pdf_rgb();
            content.push_str(&format!("{r:.4} {g:.4} {b:.4} rg\n"));
        }
        if let Some(stroke) = paint.stroke {
            let (r, g, b) = stroke.color.pdf_rgb();
            content.push_str(&format!(
                "{r:.4} {g:.4} {b:.4} RG\n{:.3} w\n{} j\n{} J\n",
                stroke.width,
                1,
                if stroke.round_cap { 1 } else { 0 }
            ));
        }
        content.push_str(&ops);
        match (paint.fill.is_some(), paint.stroke.is_some()) {
            (true, true) => content.push_str("B\n"),
            (true, false) => content.push_str("f\n"),
            (false, true) => content.push_str("S\n"),
            (false, false) => {}
        }
        content.push_str("Q\n");
    }

    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << >> /Contents 4 0 R >>",
            width_pt, height_pt
        )
        .into_bytes(),
    );
    let stream = content.into_bytes();
    let mut stream_obj = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
    stream_obj.extend_from_slice(&stream);
    if !stream_obj.ends_with(b"\n") {
        stream_obj.push(b'\n');
    }
    stream_obj.extend_from_slice(b"endstream");
    objects.push(stream_obj);
    objects.push(
        format!(
            "<< /Title (WinScore page) /Producer (WinScore Rust Edition) /Creator (WinScore Rust Edition) >>"
        )
        .into_bytes(),
    );

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize; objects.len() + 1];
    for (i, obj) in objects.iter().enumerate() {
        offsets[i + 1] = out.len();
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets[1..] {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_pos
        )
        .as_bytes(),
    );
    out
}
