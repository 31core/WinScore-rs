# 对照 `Winscore-doc/docs.zip` 的实现审计

审计日期：2026-10-30  
依据：`docs.zip` 中的 `WinScore-5.01-Spec.md`、`03-coordinates-and-page.md`、`04-layout-and-spacing.md`、`05-object-geometry.md`、`06-gui-shell.md`、`07-interaction.md`、`09-implementation-checklist.md`。

本文只记录可验证的行为差异。`【确认】` 级别的规格优先；与原版 PDF 实测冲突的地方单独注明。

## 1. 本轮已修正

| 项目 | 原实现 | 现在 | 规格依据 |
|---|---|---|---|
| 水平排版 | `J`/`LJ` 是空操作，只显示提示 | 新增 `src/layout.rs`，实现 `LI`/`J`/`LJ`/`LJA`；按 `20*|P7|*P5` 计算槽宽，和弦共槽，barline 留 3.5 槽，Justify 均分剩余宽度 | 04 §4.3–4.5 |
| 空间不足 | 无行为 | 输出 `*** Not enough space to justify ideally. Needed=…, Available=…`，按文档等量削减槽宽，保证右端不越界 | 04 §4.5.1 |
| Beam 跟随 | 排版后 Beam 的 P3/P6 仍指向旧位置 | 排版后按范围内音符重设 Beam 端点 | 04 §4.8 |
| Barline P5 | UI 把 `1` 当双线、`2` 当终止线、`4/5/6` 画法互相混淆 | UI 改为 `0/2/3/4/5/6`；重写单线、双线、终止线、左/右重复、双重复的几何 | 05 §5.13 |
| Code 7 类型 | 用 `8/15` 表示 8va/15ma，与文档的 `5/6/7/8` 不一致 | 改为 `5=8va`、`6=15ma`、`7=8vb`、`8=15mb`；补 `1=trill`、`2=mordent`、`3=inverted mordent`、`4=turn`；保留旧文件兼容 | 05 §5.7 |
| Code 8 P7 | 单线 staff 被强制画成 2 线；P11 线宽未用 | 支持 1 线、`-1` 隐藏、三位 `XYZ` 起始线/线数；P11 以 600 dpi 像素增量参与线宽 | 05 §5.8 |
| Code 8 P10 | 始终按英寸换算 | 按 trailer 的 measurement code 在英寸/厘米之间切换 | 03 §3.3.6 |
| Staff 创建界面 | P7 只能 0–200 | 可输入 `-1..999`，说明 `XYZ` 编码 | 05 §5.8 |
| 大文件头 | 写入超过 65,535 words 直接报错 | 超过 32,767 words 写 u32 头；读取端按文件长度自动判 16/32 位 | 02 §2.1、09 M0 |
| `IN` 参数 | 只接受 staff 号，其余参数忽略 | 接受 `IN a b c d e` = staff/P3/P4/P5/P6，缺失 staff 时按参数创建 | 07 §7.5.1 |
| 参数矩阵 | 只有 P1–P20 | `PM` 或参数栏按钮切换 P1–P20 / P21–P30；`PM n` 查看 Pn | 06 §6.2.1 |
| 键盘 | 方向键固定 0.5；无 Home/End/PageUp/PageDown/`[`/`]`/F2/F3/F9 | 按规格改为 1 单位/1 step，并补齐导航键 | 07 §7.3.1 |
| CLI | 只能渲染 | 增加 `--li`、`--lj`，便于批处理和回归 | — |

## 2. 仍需补齐的规格

按影响排序：

1. **跨 staff 的完整 system 排版**  
   当前 `LI`/`LJ` 对每个 staff 独立处理。原版先建立 system 的 beat 列，让所有 staff 共用列位置；这需要从 Code 14 的 P4 跨度识别 system，并实现 `LJ` 的拍数一致性检查。现在可用 `LJ 1 8` 指定范围，但跨 staff 的 barline 不保证对齐。

2. **垂直排版**  
   `STSP`、`EQA`、`EQS`、`VJ` 尚未实现。staff 的默认 `P4`、`P5`、`P10` 定位已正确，但没有“等距分配/垂直 justify”的对话框和命令。

3. **文本 Justify（`JT`）**  
   已有 Code 16 字距 P5、空格 P10、目标长度 P13 的渲染支持，但没有交互式选文本、按词/空格分配的 `JT` 流程。

4. **五遍输入法**  
   `IN` 现在能创建 staff 并进入点击输入，但没有 Pass 2–5 的 token 解析器（`4/`、`8/`、`S 1-4/`、`4b`、`1 16/17` 等），也没有 MIDI 步进输入。

5. **页布局/部件抽取**  
   `LAY`、`BARS-LINE`/`BARS.TMP`、`RIP/RIPX`、`MN`、`PARTS` 未实现。主工具栏对应按钮仍只显示说明。

6. **DRAW / EPS 导入 / PMX / PREF 读写**  
   `DRAW` 编辑器、Code 15 EPS 导入、PMX 导入导出、`PREF.DAT` 读写、`FMnn.FMT` 与 `symblist.txt` 导入尚未实现。当前使用系统字体和内置 Bravura 轮廓。

7. **DRAW 对象与罕见字形**  
   Code 9 的符号库、吉他和弦图、手风琴、Code 12 的复杂子类型只覆盖常见分支；未知 Code 会保留并可选，但不绘制。

8. **页对象/多页**  
   仅显示第一页；P2 201–256 会保留但不会与正常页合成。打印仍是单页 PDF。

## 3. 与文档冲突、暂不修改的地方

### 页面尺寸

文档 03 §3.3 写的是 US Letter `612×792 pt`，并明确指出 `554×643 pt` 是“按内容裁出的媒体框”。本实现仍使用 `554×643`，原因：

- 三个完整样本的标题、页脚和 staff 坐标恰好落在 `554×643` 内；例如 Mozart 的页脚基线在 `y≈637`，若按 Letter 的 `Vpos=733.5` 计算会落到 `y≈827`，超出纸张。
- 现有参考 PDF 的页面就是 `554×643`，导出 PNG/PDF 与该 PDF 像素级对齐。
- 文档自身也把 Letter 描述为“打印/EPS 的物理纸张”，但没有给出 `.mus` 到纸张的页偏移字段。

因此当前把 `554×643` 当作“页面媒体框”，而不是 Letter 纸张。若后续加入 `PAPER`/页边距配置，应把“媒体框”和“纸张”分开建模，而不是直接改常量。

### P7=-1 与页面高度

隐藏 staff 仍参与对象定位，但 `page_size_for` 仍只看 staff 的 P4/P10，不会因为隐藏 staff 自动缩小页面。原版用 `H`/`HS` 显式设置页面高度；该对话框尚未实现。

### Code 3 谱号映射

文档 05 §5.3 的 `P5=6/7/8`（soprano/mezzo/baritone）当前仍回退为不绘制。常见 treble/bass/alto/tenor/percussion 已覆盖。完整 C/F clef 的线位锚点需要单独校准，避免“画出来但位置错”。

## 4. 回归测试

`cargo test --all-targets` 当前 20 项：

- `src/layout.rs`：Lineup、Justify 填满、窄区间警告/等量削减、和弦共槽（4）；
- `tests/engrave.rs`：样本渲染、PDF/SVG、编辑、音频、页面尺寸、item-bar 标记、Code 8 创建、P7 单线/隐藏、P10 英制/公制、全部 barline 类型（10）；
- `tests/mus_roundtrip.rs`：三个样本 bit-exact、损坏模板、计数一致性、大文件 u32 头（4）；
- `src/model.rs`：文本解码/字体 run（2）。

排版回归直接验证：

```text
4 个四分音符（P5=1，范围 0..200）
LI  -> P3 = 0, 20, 40, 60
LJ  -> P3 = 0, 60, 120, 180
范围 0..70
LJ  -> 警告 + 等量削减，最后一个 P3 <= 70
```

## 5. 复现

```bash
CARGO_HOME=$PWD/.cargo cargo test --offline --all-targets
CARGO_HOME=$PWD/.cargo cargo run --offline --bin winscore-cli -- samples/mozart.mus out.png --lj
```
