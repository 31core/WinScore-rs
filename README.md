# WinScore 5.01 — Rust Edition

A native, independent reimplementation of **WinScore 5.01**, Leland Smith's legendary
music-engraving program (the Windows descendant of SCORE). It reads and writes the
original binary `.mus` object format, reconstructs the classic parameter-driven editing
model, and renders a letter-size page with real vector outlines.

> The original WinScore was discontinued in 2013. This project is a clean-room Rust
> implementation built from the published SCORE parameter documentation, the program's
> own help file, and byte-level analysis of the supplied `.mus` examples. No WinScore
> source code or executable is used at runtime.

---

## What works

### A real WinScore-style desktop app

* Classic light-grey, bevel-edged UI with the familiar **two colourful button banks**,
  item palette, P1–P20 parameter matrix, status/message box and SCORE-style command line.
* Open, save, save-as, recent files, and a directory browser for `.mus` files.
* Click to select any object; drag to move; arrow keys nudge; Delete removes; undo/redo.
* Edit every numeric parameter directly in the P1–P20 boxes, or use the command box:
  `3 42.5` changes P3, `L 2`/`U 1` nudges, `I 214` jumps to an object, `ST 6 1` finds
  notes on staff 6, `OS`/`OH`/`OT` reorder, `UNDO`/`REDO`, `TOP`/`MID`/`BOT`, `Z 1.5`.
* Palette tools place notes, rests, clefs, meters, key signatures, barlines, braces,
  beams, slurs, hairpins, trills, ottavas, pedals, first/second endings, text,
  dynamics, rehearsal numbers and more.
* **Create Code 8 Staff** dialog (Format menu or STAFF in the item bar) writes a real
  Code 8 staff with P2/P3/P4/P5/P6/P7/P9/P10 columns; duplicate staff numbers are
  rejected, and the page is re-engraved immediately.
* Double-click a text object to edit its contents; `_NN` WinScore font escapes and the
  `<< >> ^^ %%` accent digraphs are supported.
* Fit page / fit width, zoom, scrolling, staff number and page coordinates.
* A built-in audition synthesiser (**PLAY**) renders the engraved notes to a temporary
  WAV and plays it with the system audio player.

### Native `.mus` compatibility

* Reads the native binary SCORE/WinScore object stream: word-count header, counted
  objects, code-16 text payloads, and the six-word `-9999` trailer.
* **Bit-exact round-trips** are covered by tests for all three complete supplied scores:
  `mozart.mus`, `carmen.mus` and `sonlcs.mus`.
* Unknown object codes and unused parameters survive a load/save cycle.
* Preserves the historical, sometimes noisy, object-count words and text padding bytes.
* Editing operations write ordinary SCORE parameter blocks that WinScore can read back.

### Engraving quality

* Original WinScore 554 × 643 pt page, 7.5-inch (540 pt) music area, and the 0–200 horizontal coordinate system.
* Staff vertical placement follows the documented formula
  `(staff−1)·18 steps + P4·P5`, with absolute P10 (inch) positioning supported.
* Full object engraver for notes, chords, rests, clefs, key signatures, meters, barlines,
  braces/brackets, beams, slurs/ties, endings, tuplets, text, dynamics, numbers,
  hairpins, trills, ottavas, pedal lines, boxes/circles and articulation marks.
* **Rhythm-aware secondary beams** — a beam group of eighth + two sixteenths gets a full
  primary beam and a partial secondary beam, not a solid block.
* **Original beam geometry** — stems sit at the SCORE P3 coordinate, beams are 0.5 staff
  spaces thick and 0.75 spaces apart, and the P4/P5 reference is the outer stem tip.
  This reproduces the attachment and stacking order measured from the WinScore PDF.
* SMuFL outlines from **Bravura** and real PostScript-style text outlines, not bitmap
  symbols. Every glyph is vectorised before rendering.
* PNG at arbitrary DPI, SVG, and true **vector PDF** export. No font embedding is
  required because every glyph is emitted as outlines.

Sample renderings and PDF/SVG exports are generated locally with the commands below;
they are not checked in. The repository only keeps source, test fixtures and the
fonts/assets required to build and run.

---

## Build and run

Requirements: Rust 1.85+ on macOS (tested on macOS 27 / Apple Silicon).

```bash
cargo run --release --bin winscore                 # launch with the bundled demo
cargo run --release --bin winscore -- path/to/file.mus
```

Render a page without a GUI:

```bash
cargo run --release --bin winscore-cli -- samples/sonlcs.mus output/sonlcs.png --dpi 300
cargo run --release --bin winscore-cli -- samples/sonlcs.mus output/sonlcs.svg --svg
cargo run --release --bin winscore-cli -- samples/sonlcs.mus output/sonlcs.pdf --pdf
cargo run --release --bin winscore-cli -- samples/mozart.mus --inspect
```

Build a double-clickable app:

```bash
./scripts/bundle.sh
open dist/WinScore.app
```

The app bundle is self-contained: Bravura and the Liberation Serif fallback are compiled
into the binary. On macOS the app prefers the system Times New Roman for text and falls
back to the bundled metrically compatible Liberation Serif.

---

## Keyboard and command reference

| Input | Action |
|---|---|
| Click object | Select / edit object |
| Drag object | Move it |
| Double-click text | Edit text |
| Arrow keys | Move selected object by half a step / half a scale step |
| Shift + arrows | Same (kept for WinScore muscle memory) |
| Delete / Backspace | Delete selected object |
| Esc | Leave edit mode |
| Ctrl/Cmd + Z | Undo |
| Ctrl/Cmd + Shift + Z | Redo |
| Ctrl/Cmd + S / O | Save / open |
| F1 | Help |

The command box understands the SCORE editing idiom:

```
G FILE.MUS        SA FILE.MUS        SM
IN 6              NOTE 6            REST 6
3 42.5            4 8.5             L 2 / R 2 / U 1 / D 1
I 214             ST 6 1            OS / OH / OT
UNDO / REDO / UND / DE
TOP / MID / BOT   Z 1.5             VER / PM
```

Parameters are the published SCORE parameters. See
[`docs/MUS-FORMAT.md`](docs/MUS-FORMAT.md) for the file layout and the
parameter-code map.

---

## Project layout

```
src/model.rs      native .mus parser/writer, text digraphs, object model
src/font.rs       font loading, shaping, glyph outlines, SMuFL metrics
src/engrave.rs    SCORE-to-vector engraver and hit testing
src/draw.rs       resolution-independent paint/primitive list
src/raster.rs     tiny-skia screen/PNG renderer
src/svg.rs        SVG exporter
src/pdf.rs        self-contained vector PDF writer
src/playback.rs   score-to-WAV audition synthesiser
src/app.rs        egui desktop application and WinScore-style UI
src/bin/cli.rs    headless render/inspect utility
tests/            round-trip, engraving, export and editing tests
docs/             format notes
```

## Verification

```bash
cargo test --all-targets
```

Current tests cover:

* bit-exact `.mus` round-trips for the complete supplied samples;
* tolerant parsing of the damaged `000.mus` template without panics;
* vector export headers and page geometry;
* editing, text insertion, save/reparse and word-count consistency;
* a valid RIFF/WAVE audition export;
* engraving every supplied score without NaN coordinates and with selectable objects.

## Compatibility notes and honest limitations

* The complete supplied 1997–2013 scores load, render and round-trip exactly. The
  `000.mus` template has a damaged tail; the parser stops safely and preserves everything
  before the damage.
* Only the first page of a multi-page score is displayed. Files using the original
  verso/recto staff numbers 201–256 are preserved, but page compositing is not yet
  implemented.
* Part extraction, the DRAW editor, conditional `IF…THEN` editing, EPS import and the
  MIDI file format are not implemented in this build. Unknown objects are still kept
  byte-for-byte when saving.
* Newly placed objects are not automatically re-justified across a measure; WinScore
  pages retain their original engraved P3 positions, and manual `L/R`/drag editing is
  exact. A future release should add the full LJ/J spacing pass.
* Glyph coverage is strongest for common notation and the supplied scores. Exotic Code 9
  library entries and a handful of very rare ornaments fall back to being selectable but
  undrawn.

## Credits and licences

* Code: MIT (see [`LICENSE`](LICENSE)).
* **Bravura** music font: © Steinberg Media Technologies GmbH, SIL Open Font License 1.1
  ([`assets/fonts/OFL-Bravura.txt`](assets/fonts/OFL-Bravura.txt)).
* **Liberation Serif** fallback: SIL Open Font License
  ([`assets/fonts/OFL-Liberation.txt`](assets/fonts/OFL-Liberation.txt)).
* Times New Roman, when present, is used from the host macOS installation and is not
  redistributed.
* The WinScore help file and executable in the supplied archive were used only as
  documentation/reverse-engineering references. They are not part of this project or the
  app bundle.
