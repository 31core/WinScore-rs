use std::path::Path;

use winscore_rs::engrave::Engraver;
use winscore_rs::font::FontBook;
use winscore_rs::model::{Item, Score, TextBlock, encode_text};
use winscore_rs::{pdf, raster, svg};

#[test]
fn engraves_every_supplied_score() {
    let fonts = FontBook::load();
    for name in ["000.mus", "mozart.mus", "carmen.mus", "sonlcs.mus"] {
        let path = Path::new("samples").join(name);
        let score = Score::load(path).unwrap();
        let page = Engraver::new(&score, &fonts).engrave();
        assert!(
            page.draws.paints.len() > 50,
            "{name} produced too little artwork"
        );
        assert!(!page.hits.is_empty(), "{name} has no selectable objects");
        for p in &page.draws.paints {
            assert!(p.path.min.x.is_finite() && p.path.min.y.is_finite());
            assert!(p.path.max.x.is_finite() && p.path.max.y.is_finite());
        }
        let png = raster::render_png(&page.draws, page.width, page.height, 0.5).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }
}

#[test]
fn exports_vector_pdf_and_svg() {
    let fonts = FontBook::load();
    let score = Score::load("samples/sonlcs.mus").unwrap();
    let page = Engraver::new(&score, &fonts).engrave();
    let pdf_bytes = pdf::to_pdf(&page.draws, page.width, page.height);
    assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
    assert!(pdf_bytes.ends_with(b"%%EOF\n"));
    let svg_text = svg::to_svg(&page.draws, page.width, page.height);
    assert!(svg_text.contains("<svg"));
    assert!(svg_text.contains("viewBox=\"0 0 612.00 792.00\""));
}

#[test]
fn editing_preserves_file_structure() {
    let mut score = Score::load("samples/mozart.mus").unwrap();
    let before = score.items.len();
    let note_idx = score.items.iter().position(|i| i.code == 1).unwrap();
    let old = score.items[note_idx].p4();
    score.items[note_idx].set(4, old + 1.0);

    let mut text = Item::new(16, vec![16.0, 1.0, 100.0, 20.0, 0.0, 1.0]);
    let mut raw = b"_02".to_vec();
    raw.extend_from_slice(&encode_text("Süße Stille"));
    let raw_len = raw.len();
    text.text = Some(TextBlock {
        declared_len: raw_len,
        raw,
    });
    text.set(12, raw_len as f32);
    score.add_item(text);
    assert_eq!(score.items.len(), before + 1);

    let bytes = score.to_bytes().unwrap();
    let reread = Score::parse(&bytes).unwrap();
    assert_eq!(reread.items.len(), before + 1);
    assert_eq!(reread.items[note_idx].p4(), old + 1.0);
    assert!(reread.items.last().unwrap().text.is_some());
    assert!(
        reread
            .items
            .last()
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .display()
            .contains("Stille")
    );
    let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
    assert_eq!(bytes.len(), 2 + 4 * count);
}

#[test]
fn audio_export_is_a_valid_wave_file() {
    let score = Score::load("samples/mozart.mus").unwrap();
    let path = std::env::temp_dir().join("winscore-test.wav");
    let secs = winscore_rs::playback::export_wav(&score, &path).unwrap();
    assert!(secs > 1.0);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert!(bytes.len() > 44);
}

#[test]
fn page_sizes_follow_original_scores() {
    let fonts = FontBook::load();
    let mozart = Score::load("samples/mozart.mus").unwrap();
    let mp = Engraver::new(&mozart, &fonts).engrave();
    assert_eq!((mp.width, mp.height), (554.0, 643.0));

    let sonlcs = Score::load("samples/sonlcs.mus").unwrap();
    let sp = Engraver::new(&sonlcs, &fonts).engrave();
    assert_eq!((sp.width, sp.height), (612.0, 792.0));
}

#[test]
fn engraves_item_bar_markings() {
    let fonts = FontBook::load();
    let mut score = Score::load("samples/mozart.mus").unwrap();
    // tr~, 8va, 15m, Ped., Ped. with extension, first and second endings.
    for p7 in [0.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 20.0, 21.0, 22.0] {
        score.add_item(Item::new(7, vec![7.0, 8.0, 20.0, 13.0, 0.0, 32.0, p7]));
    }
    for n in [1.0, 2.0] {
        score.add_item(Item::new(
            5,
            vec![5.0, 8.0, 20.0, 13.0, 13.0, 36.0, -2.0, n, 0.0],
        ));
    }
    let page = Engraver::new(&score, &fonts).engrave();
    assert!(page.draws.paints.len() > 50);
    for p in &page.draws.paints {
        assert!(p.path.min.x.is_finite() && p.path.min.y.is_finite());
    }
}

#[test]
fn engraves_created_code8_staff() {
    let fonts = FontBook::load();
    let mut score = Score::load("samples/mozart.mus").unwrap();
    score.add_item(Item::new(
        8,
        vec![8.0, 9.0, 0.0, -8.0, 0.9, 200.0, 5.0, 0.0, 1.0, 0.0],
    ));
    let page = Engraver::new(&score, &fonts).engrave();
    assert!(page.staves.contains_key(&9));
    assert!(page.draws.paints.len() > 50);
}

#[test]
fn code8_line_count_and_invisible_staff_follow_p7() {
    let fonts = FontBook::load();
    let mut score = Score::load("samples/mozart.mus").unwrap();
    score.add_item(Item::new(
        8,
        vec![8.0, 9.0, 0.0, -10.0, 0.8, 200.0, 1.0, 0.0, 0.0, 0.0],
    ));
    score.add_item(Item::new(
        8,
        vec![8.0, 10.0, 0.0, -14.0, 0.8, 200.0, -1.0, 0.0, 0.0, 0.0],
    ));
    let page = Engraver::new(&score, &fonts).engrave();
    assert_eq!(page.staves[&9].lines, 1);
    assert_eq!(page.staves[&10].lines, 0);
    assert!(!page.staves[&10].visible);
}

#[test]
fn engraves_all_barline_and_repeat_types() {
    let fonts = FontBook::load();
    let mut score = Score::load("samples/mozart.mus").unwrap();
    for (i, p5) in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0].iter().enumerate() {
        score.add_item(Item::new(
            14,
            vec![14.0, 1.0, 10.0 + i as f32 * 20.0, 2.0, *p5, 0.0],
        ));
    }
    let page = Engraver::new(&score, &fonts).engrave();
    assert!(page.draws.paints.len() > 50);
    for p in &page.draws.paints {
        assert!(p.path.min.x.is_finite() && p.path.min.y.is_finite());
    }
}

#[test]
fn code8_absolute_position_uses_measurement_unit() {
    let fonts = FontBook::load();
    let mut inches = Score::default();
    inches.items.push(Item::new(
        8,
        vec![8.0, 1.0, 0.0, 0.0, 1.0, 200.0, 5.0, 0.0, 0.0, 1.0],
    ));
    let mut metric = inches.clone();
    metric.trailer.metric = true;
    let a = Engraver::new(&inches, &fonts).engrave();
    let b = Engraver::new(&metric, &fonts).engrave();
    let da = a.staves[&1].bottom - inches.trailer.undefined;
    let db = b.staves[&1].bottom - metric.trailer.undefined;
    // P10=1 inch vs P10=1 cm: the metric staff must sit higher by 72-28.35 pt.
    assert!((da - db + (72.0 - 72.0 / 2.54)).abs() < 0.01);
}

#[test]
fn auto_beam_connects_and_engraves_both_states() {
    let fonts = FontBook::load();
    let mut score = Score::default();
    score
        .items
        .push(Item::new(8, vec![8.0, 1.0, 0.0, 0.0, 1.0, 200.0, 5.0]));
    for (i, level) in [3.0, 5.0, 7.0, 6.0].iter().enumerate() {
        score.items.push(Item::new(
            1,
            vec![1.0, 1.0, 10.0 + i as f32 * 10.0, *level, 10.0, 0.0, 0.5],
        ));
    }
    let mut beam = Item::new(6, vec![6.0, 1.0, 8.0, 13.0, 13.0, 45.0, 11.0]);
    beam.set(30, 2.0);
    let beam_idx = score.add_item(beam);

    let before = Engraver::new(&score, &fonts).engrave();
    assert!(before.hits.iter().any(|h| h.item == beam_idx));
    assert!(before.draws.paints.len() > 10);

    let plan = winscore_rs::beam::plan(&score, beam_idx, false, None).unwrap();
    winscore_rs::beam::apply(&mut score, &plan);
    assert_eq!(score.items[beam_idx].get(3), 10.0);
    assert_eq!(score.items[beam_idx].get(6), 40.0);
    assert_eq!(score.items[beam_idx].get(30), 1.0);
    let after = Engraver::new(&score, &fonts).engrave();
    assert!(after.hits.iter().any(|h| h.item == beam_idx));
    assert!(after.draws.paints.len() > 10);
}

#[test]
fn engraves_line_box_circle_and_bracket_tools() {
    let fonts = FontBook::load();
    let mut score = Score::load("samples/mozart.mus").unwrap();
    score.add_item(Item::new(4, vec![4.0, 1.0, 30.0, 12.0, 12.0, 50.0, 0.0]));
    score.add_item(Item::new(
        12,
        vec![12.0, 1.0, 60.0, 12.0, 0.0, 10.0, 6.0, 1.0],
    ));
    score.add_item(Item::new(
        12,
        vec![12.0, 1.0, 80.0, 12.0, 1.0, 6.0, 6.0, 1.0],
    ));
    score.add_item(Item::new(14, vec![14.0, 1.0, 90.0, 2.0, 9.0]));
    let page = Engraver::new(&score, &fonts).engrave();
    assert!(page.draws.paints.len() > 50);
    for p in &page.draws.paints {
        assert!(p.path.min.x.is_finite() && p.path.min.y.is_finite());
    }
}
