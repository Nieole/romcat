#!/usr/bin/env python3
"""从完整的源字体裁出界面打包的三份字体子集。

界面必须**打包**字体而不是读系统字体（ADR-0005）：egui 内置字体一个汉字都没有，
而系统字体的覆盖各机器不一致——实测苹方缺 `♪`、冬青黑缺 `♪` 与 `Ⓡ`，且 macOS 上
它躺在哈希命名的路径下不可硬编码。打包子集等于把「哪些字能显示」变成一个编译期常量。

三份产物都在 `crates/gui/assets/`，由 `font.rs` 用 `include_bytes!` 嵌进二进制。这个脚本
是那三份二进制的**唯一出处**：改字符集就重跑它，别手工改字体。

| `--face` | 产物 | 源 | 字重 | 字符集 |
|---|---|---|---|---|
| `regular` | `NotoSansSC-Subset.ttf` | `NotoSansSC[wght].ttf` | 400 | GBK + Big5 + 显式补的区段 |
| `strong` | `NotoSansSC-SemiBold-Latin.ttf` | `NotoSansSC[wght].ttf` | 600 | 拉丁字母、数字与 ASCII 标点 |
| `mono` | `JetBrainsMono-Latin.ttf` | `JetBrainsMono[wght].ttf` | 400 | 拉丁字母、数字与 ASCII 标点 |

**不打包中文粗体**（2026-09-13 裁定）：中文的层次靠字号、颜色与间距，一份中文粗体要
多背 7 MB。所以 `strong` 与 `mono` 两份**一个汉字都不许有**——裁完查 `cmap`，超出字符集
就当场失败；界面上粗体族与等宽族里的中文由 `font.rs` 回退到 `regular` 那一份。
`strong` 的 600 是设计稿令牌 `[font] weight-strong` 的值，与常规体同一个源实例化，
字形设计与度量都是一套。

用法（需要 fontTools）：

    pip install fonttools
    python3 tools/font-subset.py --face regular --source /path/to/'NotoSansSC[wght].ttf'
    python3 tools/font-subset.py --face strong  --source /path/to/'NotoSansSC[wght].ttf'
    python3 tools/font-subset.py --face mono    --source /path/to/'JetBrainsMono[wght].ttf'

不给 `--face` 就是 `regular`。

源字体从 google/fonts 取：
<https://github.com/google/fonts/tree/main/ofl/notosanssc> 与
<https://github.com/google/fonts/tree/main/ofl/jetbrainsmono>。
两家都是 SIL Open Font License 1.1，但版权行不同，许可副本各一份：
`crates/gui/assets/OFL.txt`（Noto Sans SC）与 `crates/gui/assets/OFL-JetBrainsMono.txt`。

**同样的输入产出同样的字节。** 于是「仓库里那份字体是不是这个脚本裁出来的」是可核对的：
重跑一遍，对 SHA-256。两步都显式关掉了时间戳重算，否则 `head.modified` 每次都不同。
下表以 fontTools 4.64.0 实测（2026-09-13）：

| | SHA-256 | 字节 |
|---|---|---|
| 源 `NotoSansSC[wght].ttf` | `a3041811…` | 17,772,300 |
| 源 `JetBrainsMono[wght].ttf`（Version 2.211） | `48715a42…` | 187,208 |
| 产物 `NotoSansSC-Subset.ttf` | `066c7a2b…` | 7,670,804 |
| 产物 `NotoSansSC-SemiBold-Latin.ttf` | `4ae719a9…` | 28,272 |
| 产物 `JetBrainsMono-Latin.ttf` | `2961f709…` | 53,512 |
"""

from __future__ import annotations

import argparse
import dataclasses
import pathlib
import subprocess
import sys
import tempfile
from typing import Callable

# 常规体的字符集由两块拼起来：这两个双字节编码**能表示的全部字符**，加上下面几段显式补的区段。
#
# 用 Python 自带的编解码器枚举，而不是手写码位表——GBK 一家两万三千条映射，手写必错。
# GBK 是简体正文（含一部分繁体），Big5 是繁体标题：港台发行版与繁体汉化版的标题落在
# 那里，GBK 覆盖不全。
DOUBLE_BYTE_CODECS = ("gbk", "big5")

# 显式补的区段，每段的理由写在第三栏——下一个人要加字符时得知道往哪一段加。
EXTRA_RANGES = (
    (0x0020, 0x007E, "ASCII 可见字符"),
    (0x00A0, 0x017F, "拉丁补充与扩展 A：ō ū é ü ñ，日文罗马字与欧洲语标题要用"),
    (0x2000, 0x206F, "通用标点：— … “ ” ‘ ’ ※ 前后的那一段"),
    (0x20A0, 0x20BF, "货币符号：€ ₩ ₽"),
    # 这一段是这张票存在的半个理由——egui 内置字体这些符号大半都没有。
    (0x2100, 0x27BF, "符号区：Ⅲ ① ★ ☆ ♪ ♥ → Ⓡ ∀ 都在这里"),
    (0x3000, 0x303F, "中日韩标点：，。、《》〜・"),
    (0x3040, 0x30FF, "平假名与片假名"),
    (0x31F0, 0x31FF, "片假名语音扩展：小写ㇺ等，日文标题偶见"),
    (0xFE30, 0xFE4F, "中日韩兼容形式：︰ ︱ 竖排标点"),
    (0xFF00, 0xFFEF, "半角与全角形式：？ ！ ～ ￥"),
)

# 粗体与等宽两份的字符集：**只有拉丁字母、数字与 ASCII 标点**。
#
# 带变音符的拉丁字母要收：不收的话 `Pokémon` 的 `é` 在粗体里换回常规字重，一个词中间断一截。
# **拉丁补充里的标点与符号不收**（U+00A0–00BF 那一段的 `· ° © ¥ « »`，以及乘号 `×`、
# 除号 `÷`），通用标点（— … “ ”）也不收：中文标题里用的正是它们——`根 · 3 个` 里那个
# 分隔号——收进粗体的话，常规字重的一句中文中间会夹一个半粗的点。落回常规体，整句一套字重。
#
# 与 `crates/gui/tests/font.rs` 里的 `拉丁那张表里有` 是同一张表，两边都得过。
LATIN_RANGES = (
    (0x0020, 0x007E, "ASCII 可见字符：字母、数字与路径里的 - _ . / \\ :"),
    (0x00C0, 0x00D6, "拉丁补充里的字母 À–Ö"),
    (0x00D8, 0x00F6, "拉丁补充里的字母 Ø–ö（跳过乘号 ×）"),
    (0x00F8, 0x017F, "拉丁补充里的字母 ø–ÿ（跳过除号 ÷）与扩展 A：Ōkami"),
)

# 裁完必须命中的字符。少一个就说明字符集改坏了，脚本直接失败——
# 这份清单与 `crates/gui/src/font.rs` 里的 `REQUIRED` 是同一份，两边都得过。
MUST_COVER = "简繁龍鬱囧あカ，。、《》～〜・★☆♪♥Ⅲ①￥Ⓡ→∀ōé"

# 拉丁两份必须命中的：ASCII 可见字符全部（空格除外），加两个拉丁扩展的代表。
LATIN_MUST_COVER = "".join(chr(cp) for cp in range(0x21, 0x7F)) + "ōé"


def double_byte_chars(codec: str) -> set[str]:
    """枚举一个双字节编码能表示的全部字符。"""
    chars: set[str] = set()
    for lead in range(0x81, 0x100):
        for trail in range(0x40, 0x100):
            try:
                text = bytes([lead, trail]).decode(codec)
            except UnicodeDecodeError:
                continue
            # 有些码位解出替换字符或多字符串，都不要。
            if len(text) == 1 and text != "�":
                chars.add(text)
    return chars


def ranges(table: tuple[tuple[int, int, str], ...]) -> set[str]:
    return {chr(cp) for start, end, _why in table for cp in range(start, end + 1)}


def charset() -> set[str]:
    """常规体的字符集。"""
    chars: set[str] = set()
    for codec in DOUBLE_BYTE_CODECS:
        chars |= double_byte_chars(codec)
    chars |= ranges(EXTRA_RANGES)
    # 代理区不是字符，落进来会让 pyftsubset 报错。
    return {c for c in chars if not 0xD800 <= ord(c) <= 0xDFFF}


def latin_charset() -> set[str]:
    """粗体与等宽两份的字符集。"""
    return ranges(LATIN_RANGES)


@dataclasses.dataclass(frozen=True)
class Face:
    """一份产物怎么裁：从哪个源、固定到哪个字重、留哪些字、裁完查什么。"""

    source: str
    output: str
    weight: str
    chars: Callable[[], set[str]]
    must_cover: str
    # 字符集之外的码位一个都不许有：`strong` 与 `mono` 靠它守住「一个汉字都没有」。
    exact_charset: bool
    # 数字 0–9 必须一样宽：等宽体那一列数字要对得齐。
    tabular_digits: bool


FACES = {
    "regular": Face(
        source="NotoSansSC[wght].ttf",
        output="NotoSansSC-Subset.ttf",
        weight="400",
        chars=charset,
        must_cover=MUST_COVER,
        exact_charset=False,
        tabular_digits=False,
    ),
    "strong": Face(
        source="NotoSansSC[wght].ttf",
        output="NotoSansSC-SemiBold-Latin.ttf",
        weight="600",
        chars=latin_charset,
        must_cover=LATIN_MUST_COVER,
        exact_charset=True,
        tabular_digits=False,
    ),
    "mono": Face(
        source="JetBrainsMono[wght].ttf",
        output="JetBrainsMono-Latin.ttf",
        weight="400",
        chars=latin_charset,
        must_cover=LATIN_MUST_COVER,
        exact_charset=True,
        tabular_digits=True,
    ),
}


def run(argv: list[str]) -> None:
    print("$", " ".join(argv), file=sys.stderr)
    subprocess.run(argv, check=True)


def check(face: Face, output: pathlib.Path) -> list[str]:
    """裁完的产物违反了哪几条，一条一句；空表就是过了。"""
    from fontTools.ttLib import TTFont  # 延迟导入：只在核对时才需要

    problems: list[str] = []
    with TTFont(output) as font:
        cmap = font.getBestCmap()
        hmtx = font["hmtx"]
        missing = [c for c in face.must_cover if ord(c) not in cmap]
        if missing:
            problems.append(f"缺字：{''.join(missing)}")
        if face.exact_charset:
            allowed = {ord(c) for c in face.chars()}
            foreign = sorted(cp for cp in cmap if cp not in allowed)
            if foreign:
                shown = "".join(chr(cp) for cp in foreign[:20])
                problems.append(f"混进了字符集之外的 {len(foreign)} 个码位：{shown}")
        if face.tabular_digits:
            widths = {hmtx[cmap[ord(d)]][0] for d in "0123456789" if ord(d) in cmap}
            if len(widths) != 1:
                problems.append(f"数字不同宽：{sorted(widths)}")
        print(
            f"必备字符 {len(face.must_cover)} 个核过；cmap 共 {len(cmap)} 条",
            file=sys.stderr,
        )
    return problems


def main() -> int:
    here = pathlib.Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--face",
        choices=sorted(FACES),
        default="regular",
        help="裁哪一份：regular 常规体、strong 拉丁粗体、mono 拉丁等宽",
    )
    parser.add_argument(
        "--source",
        required=True,
        type=pathlib.Path,
        help="完整的源字体：regular 与 strong 用 NotoSansSC[wght].ttf，mono 用 JetBrainsMono[wght].ttf",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        help="子集字体的落点；不给就是 crates/gui/assets/ 下那一份",
    )
    parser.add_argument(
        "--weight",
        help="固定到哪个字重；可变字重会把体积翻一倍还多。不给就用这一份的默认字重",
    )
    args = parser.parse_args()
    face = FACES[args.face]
    output = args.output or here / "crates/gui/assets" / face.output
    weight = args.weight or face.weight

    if args.source.name != face.source:
        print(f"提醒：{args.face} 的源一般是 {face.source}，给的是 {args.source.name}", file=sys.stderr)

    keep = sorted(face.chars(), key=ord)
    print(f"{args.face}：字符集 {len(keep)} 个码位，字重 {weight}", file=sys.stderr)

    with tempfile.TemporaryDirectory(prefix="font-subset-") as tmp:
        work = pathlib.Path(tmp)
        codepoints = work / "codepoints.txt"
        codepoints.write_text("\n".join(f"U+{ord(c):04X}" for c in keep), "utf-8")

        # 先定字重再裁字：可变字重字体裁完仍然带着全部字重的插值数据。
        fixed = work / "fixed-weight.ttf"
        run(
            [
                sys.executable,
                "-m",
                "fontTools.varLib.instancer",
                str(args.source),
                f"wght={weight}",
                # 不盖当前时间戳，否则同样的输入每次产出的字节都不同。
                "--no-recalc-timestamp",
                "-o",
                str(fixed),
            ]
        )

        output.parent.mkdir(parents=True, exist_ok=True)
        run(
            [
                sys.executable,
                "-m",
                "fontTools.subset",
                str(fixed),
                f"--unicodes-file={codepoints}",
                f"--output-file={output}",
                # 不留 hinting：ab_glyph 不跑 TrueType 字节码，留着是纯浪费。
                "--no-hinting",
                "--drop-tables+=DSIG",
                # 同上：这两步都不盖时间戳，同样的输入才产出同样的字节——
                # 仓库里那几份二进制是否被手工动过，靠重跑一遍对哈希来核。
                "--no-recalc-timestamp",
            ]
        )

    size = output.stat().st_size
    print(f"子集：{output} {size:,} 字节（{size / 1024:.0f} KB）", file=sys.stderr)

    problems = check(face, output)
    for problem in problems:
        print(problem, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
