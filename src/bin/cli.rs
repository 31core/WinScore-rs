use std::path::PathBuf;

use anyhow::{Context, Result};
use winscore_rs::{engrave::Engraver, font::FontBook, layout, model::Score, pdf, raster, svg};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: winscore-cli <input.mus> [output.png] [--dpi N] [--svg] [--pdf] [--inspect] [--li|--lj]"
        );
        return Ok(());
    }
    let input = PathBuf::from(&args[0]);
    let mut output: Option<PathBuf> = None;
    let mut dpi = 150.0f32;
    let mut do_svg = false;
    let mut do_pdf = false;
    let mut inspect = false;
    let mut layout_mode: Option<bool> = None; // Some(false)=LI, Some(true)=LJ
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dpi" => {
                i += 1;
                dpi = args.get(i).context("--dpi needs a value")?.parse()?;
            }
            "--svg" => do_svg = true,
            "--pdf" => do_pdf = true,
            "--inspect" => inspect = true,
            "--li" => layout_mode = Some(false),
            "--lj" => layout_mode = Some(true),
            other => output = Some(PathBuf::from(other)),
        }
        i += 1;
    }

    let score = Score::load(&input).with_context(|| format!("reading {}", input.display()))?;
    for w in &score.warnings {
        eprintln!("warning: {w}");
    }
    if inspect {
        println!(
            "{} objects, {} words, version {:.2}, path {}",
            score.items.len(),
            score.to_bytes().map(|b| b.len() / 4).unwrap_or(0),
            score.trailer.version,
            input.display()
        );
        for (i, item) in score.items.iter().enumerate() {
            let txt = item.text.as_ref().map(|t| t.display()).unwrap_or_default();
            println!(
                "{:4}  code {:2}  P2={:4}  P3={:8.2} P4={:7.2}  {}",
                i + 1,
                item.code,
                item.staff(),
                item.p3(),
                item.p4(),
                txt
            );
        }
    }

    let mut score = score;
    if let Some(justify) = layout_mode {
        let staves: Vec<i32> = score
            .staff_ids()
            .into_iter()
            .filter(|s| (1..=200).contains(s))
            .collect();
        let low = staves.first().copied().unwrap_or(1);
        let high = staves.last().copied().unwrap_or(low);
        let report = layout::layout_score(&mut score, low, high, None, None, justify);
        println!("{}", report.summary());
    }
    let fonts = FontBook::load();
    let page = Engraver::new(&score, &fonts).engrave();
    if std::env::var("WS_DEBUG_PAINTS").is_ok() {
        for (i, paint) in page.draws.paints.iter().enumerate() {
            if !paint.path.empty && (paint.path.height() > 80.0 || paint.path.width() > 300.0) {
                eprintln!(
                    "paint {:4} bbox {:.1},{:.1} .. {:.1},{:.1} fill={:?} stroke={:?}",
                    i,
                    paint.path.min.x,
                    paint.path.min.y,
                    paint.path.max.x,
                    paint.path.max.y,
                    paint.fill,
                    paint.stroke.map(|s| s.width)
                );
            }
        }
    }
    let output = output.unwrap_or_else(|| {
        let stem = input.file_stem().unwrap_or_default().to_string_lossy();
        PathBuf::from(format!("{stem}.png"))
    });
    let scale = (dpi / 72.0).max(0.2);
    if do_svg {
        let out = output.with_extension("svg");
        std::fs::write(&out, svg::to_svg(&page.draws, page.width, page.height))?;
        println!("wrote {}", out.display());
    }
    if do_pdf {
        let out = output.with_extension("pdf");
        std::fs::write(&out, pdf::to_pdf(&page.draws, page.width, page.height))?;
        println!("wrote {}", out.display());
    }
    if !do_svg && !do_pdf {
        let png = raster::render_png(&page.draws, page.width, page.height, scale)?;
        std::fs::write(&output, png)?;
        println!(
            "wrote {} ({:.0} dpi, {} objects, {} paint ops)",
            output.display(),
            dpi,
            score.items.len(),
            page.draws.paints.len()
        );
    }
    Ok(())
}
