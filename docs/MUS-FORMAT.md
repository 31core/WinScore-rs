# The WinScore / SCORE binary `.mus` format

This note documents the layout used by WinScore 5.01 and verified against the four
sample files supplied with the archive. It is the specification implemented by
`src/model.rs`.

## Overall stream

```
u16  word_count                 // number of 32-bit words that follow
f32  object[0].parameter_count
f32  object[0].P1 … Pn
f32  object[1].parameter_count
…
f32  trailer[6]                // …, 5.0, -9999.0
```

* All numbers are little-endian 32-bit IEEE-754 floats, except the initial word count,
  which is a 16-bit unsigned integer in ordinary files.
* The word count *includes* the trailer and all parameter/count words, but not the
  two-byte header. For example `mozart.mus` has count 3193 and is `2 + 4·3193 = 12774`
  bytes long.
* Files larger than about 31,000 words historically switch to a 32-bit word count. The
  reader detects this fallback by checking which header size reproduces the file length.
* The final six words are:

```
0.0          undefined
serial       program serial number (2061532 in the reference files)
version      program version (3.0, 5.0, 6.0 in the samples)
measurement  0 = inches, 1 = metric
5.0          number of preceding trailer words
-9999.0      end marker
```

## Object stream

Each object is:

```
f32  parameter_count N
f32  P1        // object code
f32  P2 … PN   // parameter values
```

The object code is the integer part of P1. Missing trailing parameters are implicitly
zero. In the sample files some `N` values contain tiny fractional noise, e.g.
`5.0000152587890625`; WinScore truncates them when reading. The Rust writer preserves
the original count bits so archival round-trips are exact.

### Text objects (code 16)

A code-16 object is special. Its first 13 words are P1–P13. The remaining
`parameter_count − 13` words contain the text payload, packed four bytes per word:

```
P1=16  P2=staff  P3=x  P4=y  P5=tracking  P6=xsize  P7=ysize
P8=font override  P9=rotation  P10=space width  P11=start offset
P12=character count  P13=width in score units
then ceil(P12 / 4) words of bytes, zero/space padded
```

The payload normally begins with a three-byte font escape, `_NN`, where `NN` is one of
the 35 built-in Type 1 font numbers:

| NN | Face | NN | Face |
|---|---|---|---|
| 00 | Times-Roman | 01 | Times-Bold |
| 02 | Times-Italic | 03 | Times-BoldItalic |
| 04 | Helvetica | 05 | Helvetica-Bold |
| 06 | Helvetica-Oblique | 07 | Helvetica-BoldOblique |
| 12 | Palatino-Roman | 14 | Palatino-Italic |
| 24 | NewCenturySchlbk-Roman | 26 | NewCenturySchlbk-Italic |
| 28 | ZapfChancery-MediumItalic | 29 | Courier |

The original text input language uses ASCII digraphs for accented letters and special
characters. WinScore places the accent digraph before the vowel:

| Digraph | Result | Digraph | Result |
|---|---|---|---|
| `<<e` | é | `>>e` | è |
| `^^u` | û | `^^o` | ô |
| `%%o` | ö | `%%a` | ä |
| `?c` | ç | `?1` | é |
| `!!` | hyphenation dash | `??` | em dash |
| `\` | literal slash | | |

The Rust implementation decodes these for display and re-encodes typed Unicode text to
the same ASCII vocabulary when saving.

## Coordinates

### Horizontal

`P3` runs from 0 to 200 across 7.5 inches (540 pt).  For notes, P3 is the **stem
attachment x**, not the notehead centre; the notehead is shifted by 0.58 staff spaces
away from the stem:

```
x_stem_pt = 11.8 + P3 · 2.7
```

The 11.8 pt left margin is measured from the reference WinScore PDF.  Staff
left/right ends are Code 8 P3/P6.

### Vertical

Staff positions are numbered from the bottom of the page. The default gap between staff
bottoms is 18 diatonic steps = 0.7875 in = 56.7 pt. On the normal 554 × 643 pt WinScore
page a staff's bottom line (level 3) is:

```
y_from_bottom = 31.46 + (staff−1)·56.7 + P4·P5·3.15
```

where `P5` is the staff size (1.0 = 0.35 in staff height, 6.3 pt per space). A nonzero
P10 uses an absolute position in inches above the bottom line of staff 1 and overrides
P4. Notes and other objects use P4 as a **diatonic step** relative to the staff:

```
y_pt(level) = staff_bottom − (level − 3) · P5 · 3.15
```

Thus level 1 is middle C in treble clef, level 3 is the bottom line, level 7 the middle
line, and level 11 the top line. A staff space is **two** levels (6.3 pt at size 1);
line positions are odd levels and spaces are even levels.

## Object codes used by the engraver

| Code | Meaning | Key parameters |
|---|---|---|
| 1 | Note | P4 level, P5 stem+accidental, P6 head, P7 duration, P8 stem length, P9 dots/tails, P10 head offset (10 = one head width), P11 marks, P15 size, P19 dot vertical, P20 width, P29 accidental size, P31 dot size |
| 2 | Rest | P4 level, P5 type, P6 dots, P7 duration, P8 fermata/number, P9 centering |
| 3 | Clef | P4, P5 clef type (0 treble, 1 bass, 2 alto, 3 tenor, 4 percussion), P6 size |
| 4 | Line | P3/P4 left, P5=999 hairpin, P5/P6 right, P7 type, P8/P9 dash |
| 5 | Slur | P3/P4 left, P5/P6 right, P7 curvature, P8 type, P9 flattening |
| 6 | Beam | P3/P4 left, P5/P6 right, P7 stem direction + beam count |
| 7 | Trill/ottava/pedal | P5 size, P6 right end, P7 type |
| 8 | Staff | P3 left, P4 vertical offset, P5 size, P6 right, P7 lines, P9 instrument, P10 absolute inches |
| 9 | Symbol library | P5 symbol, P6 size, P9 rotation; see `symblist.txt` |
| 10 | Number / rehearsal mark | P5 number, P6 size, P7 font, P8 circle/box |
| 12 | Box/circle/guitar | P5 shape, P6/P7 size, P8 thickness |
| 14 | Barline/brace | P3, P4 staves covered, P5 type (0 single, 1 thin, 2 double, 3 final, 4 repeat dots left, 5 repeat dots right, 6 double repeat, 7 dashed, 8 brace, 9 bracket) |
| 16 | Text | see above |
| 17 | Key signature | P5 sharps (+) / flats (−), P6 clef type, P7 spacing |
| 18 | Meter | P5 top, P6 bottom, P7 size, P8/P9 second meter |

Notes' P5 is interpreted as:

* tens digit: 1 = stem up, 2 = stem down, absent = stemless;
* units digit: 0 none, 1 flat, 2 sharp, 3 natural, 4 double flat, 5 double sharp,
  6 quarter-flat, 7 three-quarter-flat, 8 quarter-sharp, 9 three-quarter-sharp;
* add 100 for parentheses around the accidental;
* fractional digits displace the accidental horizontally.

P9's tens digit is the number of augmentation dots; its units digit is the number of
explicit tails. P7 is duration in quarter notes (`1.0` = quarter, `0.5` = eighth,
`1.5` = dotted quarter), which is also the horizontal spacing weight.

P10 is in tenths of a head width: 10 moves a black/whole head by 1.18/1.69 staff
spaces, and values ≥100 also carry the accidental.  P31 is a multiplier for the
augmentation-dot size (`0` or `1` = normal, `1.2` = 20% larger).  P9's `+100` puts the
dot below a line and `+1000` moves it one head width to the right.

Clefs use fixed design anchors: treble G4 (P4 5), bass F3 (P4 9), alto/tenor C4
(P4 7/9); P4=1 is the standard position and the glyph moves by `(P4-1)` from there.

Beams (code 6) store the stem direction in the tens digit of P7 and the beam count in the
units digit. P4/P5 give the beam reference levels; adding/subtracting seven diatonic
steps (a normal 3.5-space stem) gives the **outer edge** of the beam stack.  This is
confirmed by the stem endpoints in the reference PDF: P3/P6 are exactly the first and
last stem x positions, and the stem tip is the outer edge of the outermost beam.  The
beam stack grows back toward the noteheads, with 0.5-space-thick beams spaced 0.75
spaces apart.  The Rust engraver uses the stored P4/P5 line when its stem clearances are
sane, falls back to the sounding notes only for stale (usually grace-note) data, and
constructs rhythm-aware partial beams.

A newly placed, not-yet-connected beam is marked with **P30=2**.  It is drawn as an
independent beam at its P3/P4–P6/P5 endpoints and does not alter stems or suppress
flags.  `AUTO`/`AUTOX` writes the connected endpoints and sets P9=-1, P30=1.  Existing
WinScore files leave P30 at 0 and keep the historical automatic stem attachment.

## Vertical staff placement example

The first staff in `mozart.mus` is at P4=24, size .9:

```
31.46 + 0 + 24 · 0.9 · 3.15 = 99.50 pt above the bottom margin
```

The second staff is at P4=25:

```
31.46 + 56.7 + 25 · 0.9 · 3.15 = 159.04 pt
```

The 59.5 pt difference is the engraved grand-staff gap. This reproduces the four
systems on the page without any hard-coded staff slots: every system is encoded by its
own Code 8 P4 offsets.  Scores whose content is too tall for 643 pt (the Leland Smith
sample) use the 612 × 792 pt letter fallback.

## Verification commands

```bash
cargo test --test mus_roundtrip
cargo run --release --bin winscore-cli -- samples/mozart.mus --inspect
```

The first test reads each complete sample, writes it again, and compares the result to
the original byte for byte.
