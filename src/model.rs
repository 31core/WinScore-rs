//! The native WinScore/SCORE `.mus` object format.
//!
//! A `.mus` file is a word stream.  The first word is a 16-bit (or, for very
//! large scores, 32-bit) count of the 32-bit words that follow.  Every object
//! is stored as a parameter count followed by its parameters; parameters are
//! little-endian IEEE-754 single precision numbers.  Text objects (code 16)
//! append their characters to the end of the parameter block, packed four
//! bytes per word.  The stream ends with a six-word trailer whose final word
//! is -9999.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TextBlock {
    /// Raw WinScore bytes, including the `_NN` font escape at the start and
    /// any word-alignment padding that was present in the original file.
    pub raw: Vec<u8>,
    /// Number of visible/declared characters (SCORE P12).
    pub declared_len: usize,
}

impl TextBlock {
    pub fn from_string(s: &str) -> Self {
        Self {
            raw: s.as_bytes().to_vec(),
            declared_len: s.len(),
        }
    }

    pub fn visible_bytes(&self) -> &[u8] {
        let n = self.declared_len.min(self.raw.len());
        &self.raw[..n]
    }

    pub fn display(&self) -> String {
        decode_text(self.visible_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    /// P1: object code (1 = note, 2 = rest, ... 18 = meter).
    pub code: i32,
    /// Numeric parameters P1...Pn.
    pub p: Vec<f32>,
    /// Code-16 text payload.
    pub text: Option<TextBlock>,
    /// The original count word.  A few historical files contain a count with
    /// a tiny fractional error; preserving its bits keeps round-trips exact.
    pub raw_count: Option<f32>,
}

impl Item {
    pub fn new(code: i32, p: Vec<f32>) -> Self {
        Self {
            code,
            p,
            text: None,
            raw_count: None,
        }
    }

    #[inline]
    pub fn get(&self, n: usize) -> f32 {
        if n == 0 {
            return 0.0;
        }
        self.p.get(n - 1).copied().unwrap_or(0.0)
    }

    #[inline]
    pub fn set(&mut self, n: usize, v: f32) {
        if n == 0 {
            return;
        }
        if self.p.len() < n {
            self.p.resize(n, 0.0);
        }
        self.p[n - 1] = v;
    }

    #[inline]
    pub fn staff(&self) -> i32 {
        self.get(2).round() as i32
    }

    #[inline]
    pub fn p3(&self) -> f32 {
        self.get(3)
    }

    #[inline]
    pub fn p4(&self) -> f32 {
        self.get(4)
    }

    pub fn label(&self) -> &'static str {
        match self.code {
            1 => "Note",
            2 => "Rest",
            3 => "Clef",
            4 => "Line",
            5 => "Slur",
            6 => "Beam",
            7 => "Trill/Ottava",
            8 => "Staff",
            9 => "Symbol",
            10 => "Number",
            11 => "DRW symbol",
            12 => "Box/Guitar",
            13 => "(unused)",
            14 => "Barline",
            15 => "EPS",
            16 => "Text",
            17 => "Key signature",
            18 => "Meter",
            _ => "Object",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Trailer {
    pub undefined: f32,
    pub serial: f32,
    pub version: f32,
    pub metric: bool,
    pub count: f32,
}

impl Default for Trailer {
    fn default() -> Self {
        Self {
            undefined: 0.0,
            serial: 2061532.0,
            version: 5.01,
            metric: false,
            count: 5.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Score {
    pub items: Vec<Item>,
    pub trailer: Trailer,
    pub path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum MusError {
    Io(std::io::Error),
    TooShort,
    BadHeader,
    TooLarge(usize),
}

impl fmt::Display for MusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MusError::Io(e) => write!(f, "I/O error: {e}"),
            MusError::TooShort => write!(f, "file is too short to be a WinScore .mus file"),
            MusError::BadHeader => write!(f, "unrecognised WinScore .mus header"),
            MusError::TooLarge(n) => write!(
                f,
                "score has {n} words; the classic format is limited to 65535"
            ),
        }
    }
}

impl std::error::Error for MusError {}

impl Score {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, MusError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(MusError::Io)?;
        let mut s = Self::parse(&bytes)?;
        s.path = Some(path.to_path_buf());
        Ok(s)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, MusError> {
        if bytes.len() < 8 {
            return Err(MusError::TooShort);
        }

        // Standard files begin with a 16-bit word count.  WinScore switches to
        // a 32-bit count for files with more than about 31,000 words.  The
        // 32-bit form is recognised by the fact that the 16-bit interpretation
        // cannot describe the file length.
        let n16 = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let (header_len, n) = if bytes.len() == 2 + 4 * n16 {
            (2usize, n16)
        } else {
            let n32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
            if bytes.len() >= 4 + 4 * n32 && n32 > 0 {
                (4usize, n32)
            } else if bytes.len() >= 2 + 4 * n16 {
                // Be liberal with a few bytes of historical padding.
                (2usize, n16)
            } else {
                return Err(MusError::BadHeader);
            }
        };

        let mut warnings = Vec::new();
        let mut words = Vec::with_capacity(n);
        for i in 0..n {
            let off = header_len + i * 4;
            if off + 4 > bytes.len() {
                warnings.push(format!("file ends early at word {i}"));
                break;
            }
            words.push(f32::from_le_bytes([
                bytes[off],
                bytes[off + 1],
                bytes[off + 2],
                bytes[off + 3],
            ]));
        }

        let mut trailer = Trailer::default();
        let mut content_end = words.len();
        if words.len() >= 6 && (words[words.len() - 1] + 9999.0).abs() < 0.01 {
            let t = &words[words.len() - 6..];
            trailer = Trailer {
                undefined: t[0],
                serial: t[1],
                version: t[2],
                metric: t[3] > 0.5,
                count: t[4],
            };
            content_end -= 6;
        } else {
            warnings.push("trailer (-9999) not found; file may be truncated".into());
        }

        let mut items = Vec::new();
        let mut i = 0usize;
        let mut broken = false;
        while i < content_end {
            let count_f = words[i];
            if !count_f.is_finite() || count_f < 1.0 || count_f > 4096.0 {
                warnings.push(format!("bad object length at word {i}"));
                broken = true;
                break;
            }
            let count = count_f as usize;
            if i + 1 + count > content_end {
                warnings.push(format!(
                    "object at word {i} extends past the end of the score"
                ));
                broken = true;
                break;
            }
            let params = &words[i + 1..i + 1 + count];
            let code = params[0] as i32;
            let mut item = Item {
                code,
                p: params.to_vec(),
                text: None,
                raw_count: Some(count_f),
            };

            if code == 16 && count > 13 {
                // P1..P13 are numeric, then P12 bytes of text packed into the
                // remaining words.  Characters after P12 are alignment padding.
                let text_words = count - 13;
                let start = header_len + (i + 1 + 13) * 4;
                let available = text_words * 4;
                let declared = item.get(12).max(0.0) as usize;
                if start < bytes.len() {
                    // Keep the full word-padded payload: historical files use
                    // spaces (not only NULs) as padding and exact round-trips
                    // matter to archivists.
                    let end = (start + available).min(bytes.len());
                    let raw = bytes[start..end].to_vec();
                    item.text = Some(TextBlock {
                        raw,
                        declared_len: declared,
                    });
                }
            }

            items.push(item);
            i += 1 + count;
        }
        if broken {
            warnings.push("remaining objects were ignored".into());
        }

        Ok(Self {
            items,
            trailer,
            path: None,
            warnings,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, MusError> {
        // The header is rewritten after the stream has been counted.  Reserve
        // four bytes so both the historical u16 header and the u32 form used
        // by large scores can be emitted without moving the object data.
        let mut out = vec![0u8, 0u8, 0u8, 0u8];
        let mut word_count = 0usize;

        for item in &self.items {
            if let Some(text) = &item.text {
                let mut p = item.p.clone();
                p.truncate(13);
                while p.len() < 13 {
                    p.push(0.0);
                }
                let mut raw = text.raw.clone();
                if p[11] <= 0.0 && !raw.is_empty() {
                    p[11] = raw.len() as f32;
                }
                let text_words = raw.len().div_ceil(4);
                let count = 13 + text_words;
                let count_word = item
                    .raw_count
                    .filter(|rc| (rc.round() as usize) == count)
                    .unwrap_or(count as f32);
                push_f32(&mut out, count_word);
                for v in &p {
                    push_f32(&mut out, *v);
                }
                raw.resize(text_words * 4, 0);
                out.extend_from_slice(&raw);
                word_count += 1 + count;
            } else {
                let count = item.p.len();
                let count_word = item.raw_count.unwrap_or(count as f32);
                push_f32(&mut out, count_word);
                for v in &item.p {
                    push_f32(&mut out, *v);
                }
                word_count += 1 + count;
            }
        }

        for v in [
            self.trailer.undefined,
            self.trailer.serial,
            self.trailer.version,
            if self.trailer.metric { 1.0 } else { 0.0 },
            self.trailer.count.max(5.0),
            -9999.0,
        ] {
            push_f32(&mut out, v);
        }
        word_count += 6;

        if word_count > i16::MAX as usize {
            if word_count > u32::MAX as usize {
                return Err(MusError::TooLarge(word_count));
            }
            // Section 2.1: large files replace the u16 count with a u32.  The
            // low half is stored first, matching the little-endian float
            // stream that follows.
            out[0..4].copy_from_slice(&(word_count as u32).to_le_bytes());
        } else {
            out[0..2].copy_from_slice(&(word_count as u16).to_le_bytes());
            out.drain(2..4);
        }
        Ok(out)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), MusError> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes).map_err(MusError::Io)
    }

    pub fn save_copy(&self, path: impl AsRef<Path>) -> Result<(), MusError> {
        let mut clone = self.clone();
        clone.path = Some(path.as_ref().to_path_buf());
        clone.save(path)
    }

    pub fn staff_ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self
            .items
            .iter()
            .filter(|i| i.code == 8)
            .map(|i| i.staff())
            .filter(|n| (1..=256).contains(n))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn highest_staff(&self) -> i32 {
        self.items
            .iter()
            .map(|i| i.staff())
            .filter(|n| (1..=256).contains(n))
            .max()
            .unwrap_or(1)
    }

    /// Find the closest object to a page position, preferring nearby objects
    /// on the closest staff.
    pub fn closest_item(&self, code_filter: &[i32]) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| code_filter.is_empty() || code_filter.contains(&it.code))
            .max_by(|(_, a), (_, b)| {
                a.get(3)
                    .partial_cmp(&b.get(3))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        a.get(4)
                            .partial_cmp(&b.get(4))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            })
            .map(|(i, _)| i)
    }

    pub fn add_item(&mut self, item: Item) -> usize {
        self.items.push(item);
        self.items.len() - 1
    }

    pub fn remove_item(&mut self, index: usize) -> Option<Item> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// Keep a stable, musically sensible display order.  WinScore's OS command
    /// sorts by staff, then by horizontal position, but preserving the original
    /// order for ties and engraving support is important for round-tripping.
    pub fn reorder_horizontal(&mut self) {
        self.items.sort_by(|a, b| {
            b.staff()
                .cmp(&a.staff())
                .then_with(|| {
                    a.get(3)
                        .partial_cmp(&b.get(3))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    a.get(4)
                        .partial_cmp(&b.get(4))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    }
}

fn push_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// WinScore's typewriter-era transliteration.  The source files use plain
/// ASCII digraphs for accented characters and a handful of typographic marks.
pub fn decode_text(raw: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0usize;
    while i < raw.len() {
        let b = raw[i];
        // Font selector: _NN.  It is not part of the visible string.
        if b == b'_'
            && i + 2 < raw.len()
            && raw[i + 1].is_ascii_digit()
            && raw[i + 2].is_ascii_digit()
        {
            i += 3;
            continue;
        }
        // WinScore places an accent digraph before the vowel it modifies.
        if i + 2 < raw.len() {
            let triple = match (raw[i], raw[i + 1], raw[i + 2].to_ascii_lowercase()) {
                (b'<', b'<', b'e') => Some('é'),
                (b'>', b'>', b'e') => Some('è'),
                (b'^', b'^', b'a') => Some('â'),
                (b'^', b'^', b'e') => Some('ê'),
                (b'^', b'^', b'i') => Some('î'),
                (b'^', b'^', b'o') => Some('ô'),
                (b'^', b'^', b'u') => Some('û'),
                (b'%', b'%', b'o') => Some('ö'),
                (b'%', b'%', b'a') => Some('ä'),
                (b'%', b'%', b'u') => Some('ü'),
                _ => None,
            };
            if let Some(c) = triple {
                s.push(c);
                i += 3;
                continue;
            }
        }
        if i + 1 < raw.len() {
            let pair = (raw[i], raw[i + 1]);
            let replacement = match pair {
                (b'<', b'<') => Some('é'),
                (b'>', b'>') => Some('è'),
                (b'^', b'^') => Some('ü'),
                (b'%', b'%') => Some('ö'),
                (b'!', b'!') => Some('-'),
                (b'?', b'?') => Some('—'),
                (b'?', b'c') => Some('ç'),
                (b'?', b'C') => Some('Ç'),
                (b'?', b'1') => Some('é'),
                (b'?', b'2') => Some('è'),
                (b'?', b'3') => Some('ê'),
                (b'?', b'4') => Some('à'),
                (b'?', b'5') => Some('ù'),
                (b'|', b'|') => Some('—'),
                _ => None,
            };
            if let Some(c) = replacement {
                s.push(c);
                i += 2;
                continue;
            }
        }
        if b == b'\\' {
            // SCORE's escaped slash.
            s.push('/');
        } else if b >= 0x20 || b == b'\t' {
            // The old files are effectively CP1252 in their upper range.
            s.push(b as char);
        }
        i += 1;
    }
    s.trim_end().to_string()
}

/// Encode typed Unicode text into WinScore's ASCII transliteration.  The
/// original program could only place the glyphs in its Type 1 font set, and
/// its input language used digraphs for the common accented letters.
pub fn encode_text(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'é' | 'É' => out.extend_from_slice(if ch == 'É' { b"?1" } else { b"<<" }),
            'è' | 'È' => out.extend_from_slice(if ch == 'È' { b"?2" } else { b">>" }),
            'ê' | 'Ê' => out.extend_from_slice(b"?3"),
            'à' | 'À' => out.extend_from_slice(b"?4"),
            'ù' | 'Ù' => out.extend_from_slice(b"?5"),
            'ü' | 'Ü' => out.extend_from_slice(b"^^"),
            'ö' | 'Ö' => out.extend_from_slice(b"%%"),
            'ç' | 'Ç' => out.extend_from_slice(if ch == 'Ç' { b"?C" } else { b"?c" }),
            '–' | '—' => out.extend_from_slice(b"??"),
            '\t' => out.push(b'\t'),
            '\n' => out.push(b' '),
            c if (c as u32) >= 32 && (c as u32) < 127 => out.push(c as u8),
            c if (c as u32) >= 160 && (c as u32) <= 255 => out.push(c as u8),
            _ => out.push(b'?'),
        }
    }
    out
}

/// Split a text payload into (font number, printable text) runs.  Font escapes
/// may occur anywhere in a string, not just at the beginning.
pub fn text_runs(raw: &[u8]) -> Vec<(u8, String)> {
    let mut runs: Vec<(u8, String)> = Vec::new();
    let mut font = 0u8;
    let mut cur = String::new();
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == b'_'
            && i + 2 < raw.len()
            && raw[i + 1].is_ascii_digit()
            && raw[i + 2].is_ascii_digit()
        {
            if !cur.is_empty() {
                runs.push((font, std::mem::take(&mut cur)));
            }
            font = (raw[i + 1] - b'0') * 10 + (raw[i + 2] - b'0');
            i += 3;
            continue;
        }
        if i + 2 < raw.len() {
            let triple = match (raw[i], raw[i + 1], raw[i + 2].to_ascii_lowercase()) {
                (b'<', b'<', b'e') => Some('é'),
                (b'>', b'>', b'e') => Some('è'),
                (b'^', b'^', b'a') => Some('â'),
                (b'^', b'^', b'e') => Some('ê'),
                (b'^', b'^', b'i') => Some('î'),
                (b'^', b'^', b'o') => Some('ô'),
                (b'^', b'^', b'u') => Some('û'),
                (b'%', b'%', b'o') => Some('ö'),
                (b'%', b'%', b'a') => Some('ä'),
                (b'%', b'%', b'u') => Some('ü'),
                _ => None,
            };
            if let Some(c) = triple {
                cur.push(c);
                i += 3;
                continue;
            }
        }
        if i + 1 < raw.len() {
            let pair = &raw[i..i + 2];
            if let Some(c) = match pair {
                b"<<" => Some('é'),
                b">>" => Some('è'),
                b"^^" => Some('ü'),
                b"%%" => Some('ö'),
                b"!!" => Some('-'),
                b"??" => Some('—'),
                b"?c" => Some('ç'),
                b"?C" => Some('Ç'),
                b"?1" => Some('é'),
                b"?2" => Some('è'),
                b"?3" => Some('ê'),
                b"?4" => Some('à'),
                b"?5" => Some('ù'),
                b"||" => Some('—'),
                _ => None,
            } {
                cur.push(c);
                i += 2;
                continue;
            }
        }
        let b = raw[i];
        if b == b'\\' {
            cur.push('/');
        } else if b >= 0x20 || b == b'\t' {
            cur.push(b as char);
        }
        i += 1;
    }
    if !cur.is_empty() {
        runs.push((font, cur));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_decode_strips_font_escape() {
        assert_eq!(decode_text(b"_02dolce"), "dolce");
        assert_eq!(decode_text(b"_00Fl^^ute"), "Flûte");
        assert_eq!(decode_text(b"_00Violons  "), "Violons");
    }

    #[test]
    fn text_runs_switch_fonts() {
        let r = text_runs(b"_00A stitch _02in time");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, 0);
        assert_eq!(r[1].1, "in time");
    }
}
