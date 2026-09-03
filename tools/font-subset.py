#!/usr/bin/env python3
"""从完整的 Noto Sans SC 裁出界面用的字体子集。

界面必须**打包**字体而不是读系统字体（ADR-0005）：egui 内置字体一个汉字都没有，
而系统字体的覆盖各机器不一致——实测苹方缺 `♪`、冬青黑缺 `♪` 与 `Ⓡ`，且 macOS 上
它躺在哈希命名的路径下不可硬编码。打包子集等于把「哪些字能显示」变成一个编译期常量。

产物是 `crates/gui/assets/NotoSansSC-Subset.ttf`，由 `font.rs` 用 `include_bytes!` 嵌进
二进制。这个脚本是那份二进制的**出处**：改字符集就重跑它，别手工改字体。

用法（需要 fontTools）：

    pip install fonttools
    python3 tools/font-subset.py --source /path/to/'NotoSansSC[wght].ttf'

源字体从 <https://github.com/google/fonts/tree/main/ofl/notosanssc> 取，
许可为 SIL Open Font License 1.1，副本在 `crates/gui/assets/OFL.txt`。

**同样的输入产出同样的字节。** 于是「仓库里那份字体是不是这个脚本裁出来的」是可核对的：
重跑一遍，对 SHA-256。两步都显式关掉了时间戳重算，否则 `head.modified` 每次都不同。

| | SHA-256 | 字节 |
|---|---|---|
| 源 `NotoSansSC[wght].ttf` | `a3041811…` | 17,772,300 |
| 产物 `NotoSansSC-Subset.ttf` | `066c7a2b…` | 7,670,804 |
"""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys
import tempfile

# 字符集由两块拼起来：这两个双字节编码**能表示的全部字符**，加上下面几段显式补的区段。
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

# 裁完必须命中的字符。少一个就说明字符集改坏了，脚本直接失败——
# 这份清单与 `crates/gui/src/font.rs` 里的 `REQUIRED` 是同一份，两边都得过。
MUST_COVER = "简繁龍鬱囧あカ，。、《》～〜・★☆♪♥Ⅲ①￥Ⓡ→∀ōé"


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


def charset() -> set[str]:
    chars: set[str] = set()
    for codec in DOUBLE_BYTE_CODECS:
        chars |= double_byte_chars(codec)
    for start, end, _why in EXTRA_RANGES:
        chars |= {chr(cp) for cp in range(start, end + 1)}
    # 代理区不是字符，落进来会让 pyftsubset 报错。
    return {c for c in chars if not 0xD800 <= ord(c) <= 0xDFFF}


def run(argv: list[str]) -> None:
    print("$", " ".join(argv), file=sys.stderr)
    subprocess.run(argv, check=True)


def main() -> int:
    here = pathlib.Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        required=True,
        type=pathlib.Path,
        help="完整的 NotoSansSC[wght].ttf",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=here / "crates/gui/assets/NotoSansSC-Subset.ttf",
        help="子集字体的落点",
    )
    parser.add_argument(
        "--weight",
        default="400",
        help="固定到哪个字重；可变字重会把体积翻一倍还多",
    )
    args = parser.parse_args()

    keep = sorted(charset(), key=ord)
    print(f"字符集：{len(keep)} 个码位", file=sys.stderr)

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
                f"wght={args.weight}",
                # 不盖当前时间戳，否则同样的输入每次产出的字节都不同。
                "--no-recalc-timestamp",
                "-o",
                str(fixed),
            ]
        )

        args.output.parent.mkdir(parents=True, exist_ok=True)
        run(
            [
                sys.executable,
                "-m",
                "fontTools.subset",
                str(fixed),
                f"--unicodes-file={codepoints}",
                f"--output-file={args.output}",
                # 不留 hinting：ab_glyph 不跑 TrueType 字节码，留着是纯浪费。
                "--no-hinting",
                "--drop-tables+=DSIG",
                # 同上：这两步都不盖时间戳，同样的输入才产出同样的字节——
                # 这份 7.3 MB 的二进制是否被手工动过，靠重跑一遍对哈希来核。
                "--no-recalc-timestamp",
            ]
        )

    size = args.output.stat().st_size
    print(f"子集：{args.output} {size:,} 字节（{size / 1024 / 1024:.1f} MB）", file=sys.stderr)

    from fontTools.ttLib import TTFont  # 延迟导入：只在核对时才需要

    with TTFont(args.output) as font:
        cmap = font.getBestCmap()
    missing = [c for c in MUST_COVER if ord(c) not in cmap]
    if missing:
        print(f"缺字：{''.join(missing)}", file=sys.stderr)
        return 1
    print(f"必备字符 {len(MUST_COVER)} 个全部命中；cmap 共 {len(cmap)} 条", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
