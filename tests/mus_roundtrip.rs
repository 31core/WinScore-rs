use std::path::Path;

use winscore_rs::model::Score;

#[test]
fn roundtrips_clean_samples() {
    for name in ["mozart.mus", "carmen.mus", "sonlcs.mus"] {
        let path = Path::new("samples").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let score = Score::parse(&bytes).unwrap();
        assert!(score.items.len() > 100, "{name}: {}", score.items.len());
        let out = score.to_bytes().unwrap();
        if out != bytes {
            let n = bytes.len().min(out.len());
            let pos = (0..n).find(|&i| bytes[i] != out[i]);
            panic!(
                "{name} did not round-trip; first difference at {pos:?}, in {} out {}\norig {:?}\nout  {:?}",
                bytes.len(),
                out.len(),
                pos.map(|i| &bytes[i.saturating_sub(8)..(i + 12).min(bytes.len())]),
                pos.map(|i| &out[i.saturating_sub(8)..(i + 12).min(out.len())])
            );
        }
    }
}

#[test]
fn parses_damaged_template_without_panicking() {
    let bytes = std::fs::read("samples/000.mus").unwrap();
    let score = Score::parse(&bytes).unwrap();
    assert!(!score.items.is_empty());
}

#[test]
fn counts_are_consistent_for_all_samples() {
    for name in ["000.mus", "mozart.mus", "carmen.mus", "sonlcs.mus"] {
        let path = Path::new("samples").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let score = Score::parse(&bytes).unwrap();
        let out = score.to_bytes().unwrap();
        let count = u16::from_le_bytes([out[0], out[1]]) as usize;
        assert_eq!(out.len(), 2 + count * 4, "{name}");
    }
}
