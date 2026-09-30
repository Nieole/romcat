"""核对设计稿 prototype.html 与令牌是否一致：顶部的 CSS 变量，以及写在规则上的字面值。

令牌全仓库只有一份：crates/gui/src/tokens.toml——界面编进二进制的就是它。设计稿目录里
不留第二份：两份各改各的，正是这份脚本要防的事。

**漏核也算不一致**（票 gate-and-tests/07）：令牌文件里每一格，要么被下面某条核对取去与设计稿比过，
要么列在文件末尾的豁免表 EXEMPT 里、写明为什么设计稿里没有可核的对应物；两样都不沾的，
照样退 1 并说出是哪几格。新立一个令牌，就在这儿补一条核对，或者进豁免表。

用法：python3 check_tokens.py [令牌文件]
    在哪个目录下运行都行；令牌文件缺省是 crates/gui/src/tokens.toml。不一致时列出差异并以 1 退出。
"""
import re
import sys
import tomllib
from pathlib import Path

here = Path(__file__).resolve().parent
repo = here.parents[1]
tokens_path = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else repo / "crates" / "gui" / "src" / "tokens.toml"
raw_tokens = tomllib.loads(tokens_path.read_text(encoding="utf-8"))
html = (here / "prototype.html").read_text(encoding="utf-8")

# ── 记账：哪几格令牌被核对取过 ──
# 下面每条核对都从 tokens 里取令牌值去与设计稿比。tokens 是套在令牌外面的一层记账壳：取过哪一格，
# 就在 covered 里记下那一格的名字（节.键，数组逐格记成 节.键[i]）。只列键名（for k in 节）不算取过。
# 全部核对跑完，令牌文件里还有没取过、也不在 EXEMPT 里的格子，就是没人核——见文件末尾。
covered: set[str] = set()


class Section(dict):
    """令牌里的一节。按键取值与 .items() 记账；只数键名（for、in、len）不记。别的 dict 方法没覆写，取了也不记——
    那只会让那一格被报成没人核，不会把没核的算成核过。"""

    def __init__(self, path: str, data: dict):
        super().__init__({key: watched(f"{path}{key}", value) for key, value in data.items()})
        self.path = path

    def __getitem__(self, key):
        value = super().__getitem__(key)
        if not isinstance(value, (Section, Row)):
            covered.add(self.path + key)
        return value

    def items(self):
        return [(key, self[key]) for key in self]


class Row(list):
    """令牌里的一个数组。按下标取、遍历、整个拿去比都记账——记的是哪几格，不是整个键。"""

    def __init__(self, path: str, data: list):
        super().__init__(data)
        self.path = path

    def _all(self):
        covered.update(f"{self.path}[{i}]" for i in range(len(self)))

    def __getitem__(self, at):
        if isinstance(at, slice):
            covered.update(f"{self.path}[{i}]" for i in range(*at.indices(len(self))))
        else:
            covered.add(f"{self.path}[{at % len(self)}]")
        return super().__getitem__(at)

    def __iter__(self):
        self._all()
        return super().__iter__()

    def __eq__(self, other):
        self._all()
        return list.__eq__(self, other)

    def __ne__(self, other):
        self._all()
        return list.__ne__(self, other)

    __hash__ = None


def watched(path: str, value):
    if isinstance(value, dict):
        return Section(path + ".", value)
    if isinstance(value, list):
        return Row(path, value)
    return value


tokens = Section("", raw_tokens)


def css_block(selector_pattern: str) -> dict[str, str]:
    m = re.search(selector_pattern + r"\{(.*?)\}", html, re.S)
    if not m:
        sys.exit(f"找不到样式块：{selector_pattern}")
    return dict(re.findall(r"--([\w-]+):\s*([^;]+);", m.group(1)))


def norm(v: str) -> str:
    v = v.strip()
    m = re.fullmatch(r"rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)", v)
    if m:
        r, g, b, a = int(m[1]), int(m[2]), int(m[3]), float(m[4])
        return f"#{r:02X}{g:02X}{b:02X}{round(a * 255):02X}"
    if m := re.fullmatch(r"#([0-9A-Fa-f])([0-9A-Fa-f])([0-9A-Fa-f])", v):
        return "#" + "".join(c * 2 for c in m.groups()).upper()
    return v.upper()


def same_color(got: str, want: str) -> bool:
    """设计稿里的一色与令牌比。半透明的允许 1 级取整误差（设计稿写 rgba 的小数，令牌写 #RRGGBBAA）。"""
    a, b = norm(got), norm(want)
    return a == b or (len(a) == len(b) == 9 and a[:7] == b[:7] and abs(int(a[7:], 16) - int(b[7:], 16)) <= 1)


problems = []
for theme, pattern in [("light", r":root"), ("dark", r':root\[data-theme="dark"\]')]:
    css = css_block(pattern)
    for key, want in tokens["color"][theme].items():
        got = css.get(key)
        if got is None:
            problems.append(f"{theme}: CSS 里没有 --{key}")
        elif not same_color(got, want):
            problems.append(f"{theme}: --{key} 设计稿是 {got}，令牌是 {want}")

pcol = dict(re.findall(r"(\w+):'(#[0-9A-Fa-f]{6})'", re.search(r"const PCOL=\{(.*?)\}", html).group(1)))
platform_colors = tokens["color"]["platform"]
for plat in platform_colors:
    if plat == "other":
        continue
    want = platform_colors[plat]
    if pcol.get(plat, "").upper() != want.upper():
        problems.append(f"平台色 {plat}: 设计稿是 {pcol.get(plat)}，令牌是 {want}")

radius = css_block(r":root")
for name, css_key in [("small", "r-s"), ("medium", "r"), ("large", "r-l")]:
    if radius.get(css_key) != f"{tokens['radius'][name]}px":
        problems.append(f"圆角 {name}: 设计稿是 {radius.get(css_key)}，令牌是 {tokens['radius'][name]}px")

# 按钮三档的高与左右留白：设计稿 .btn / .btn.sm / .btn.lg 里写的是字面值，不是 CSS 变量，照字面值核。
buttons = 0
for selector, prefix in [(r"\.btn\{", "button"), (r"\.btn\.sm\{", "button-small"), (r"\.btn\.lg\{", "button-large")]:
    m = re.search(selector + r"[^}]*?height:(\d+)px;padding:0 (\d+)px", html)
    if not m:
        problems.append(f"找不到 {selector} 的 height 与 padding")
        continue
    for got, key in [(m[1], f"{prefix}-height"), (m[2], f"{prefix}-padding")]:
        buttons += 1
        if int(got) != tokens["layout"][key]:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {tokens['layout'][key]}")

# 单行输入框：左右留白照设计稿 .input 的 padding。高**不照** .input 的 30px——拿主意的人 2026-09-14
# 第三次裁：与同一行那一档按钮等高。所以核的是「等高」这条，不是那个 30。
literals = 0
m = re.search(r"\.input\{[^}]*?height:(\d+)px;[^}]*?padding:0 (\d+)px", html)
if not m:
    problems.append("找不到 .input 的 height 与 padding")
else:
    literals += 1
    if int(m[2]) != tokens["layout"]["input-padding"]:
        problems.append(f"input-padding: 设计稿是 {m[2]}px，令牌是 {tokens['layout']['input-padding']}")
for input_key, button_key in [("input-height", "button-height"), ("input-small-height", "button-small-height")]:
    literals += 1
    if tokens["layout"][input_key] != tokens["layout"][button_key]:
        problems.append(f"{input_key} 是 {tokens['layout'][input_key]}，与同档按钮 {button_key} 的 {tokens['layout'][button_key]} 不等高")

# 开场主库列表每一行（设计稿 .catrow）：竖向间距与内边距。
m = re.search(r"\.catrow\{[^}]*?gap:(\d+)px \d+px;padding:(\d+)px (\d+)px", html)
if not m:
    problems.append("找不到 .catrow 的 gap 与 padding")
else:
    row_padding = tokens["space"]["catalog-row-padding"]
    for got, want, key in [(m[1], tokens["space"]["catalog-row-gap"], "catalog-row-gap"), (m[2], row_padding[0], "catalog-row-padding 上下"), (m[3], row_padding[1], "catalog-row-padding 左右")]:
        literals += 1
        if int(got) != want:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {want}")

# 标签左边那枚圆点（设计稿 .chip::before）：直径。
m = re.search(r"\.chip::before\{[^}]*?width:(\d+)px", html)
if not m:
    problems.append("找不到 .chip::before 的 width")
else:
    literals += 1
    if int(m[1]) != tokens["layout"]["chip-dot"]:
        problems.append(f"chip-dot: 设计稿是 {m[1]}px，令牌是 {tokens['layout']['chip-dot']}")

# 浏览屏（票 gui-looks-like-the-design/09）：设计稿写在规则上、表头上的字面量，照字面值一个个核。
# （函数叫 literal_pairs：下面库屏那一段另有一个 literal，签名与用法不同，两个分开叫。）
L, S, F, M = tokens["layout"], tokens["space"], tokens["font"], tokens["mix"]


def literal_value(got: str) -> float:
    """设计稿里的一个字面值折成数：带 % 的按成数（7% → 0.07），其余照原样。"""
    return float(got[:-1]) / 100 if got.endswith("%") else float(got)


def literal_pairs(label, pattern, pairs):
    """在设计稿里按 pattern 找一处，逐组与令牌比。pairs 是 [(组号, 令牌值, 叫什么)]；带 % 的按成数比。"""
    global literals
    m = re.search(pattern, html)
    if not m:
        problems.append(f"找不到 {label}")
        return
    for group, want, key in pairs:
        literals += 1
        got = m[group]
        if abs(literal_value(got) - float(want)) > 1e-9:
            problems.append(f"{key}: 设计稿是 {got}，令牌是 {want}")


literal_pairs(".fpane", r"\.fpane\{[^}]*?padding:(\d+)px;[^}]*?gap:(\d+)px", [(1, S["filter-pane-padding"], "filter-pane-padding"), (2, S["pane-gap"], "pane-gap")])
literal_pairs(".dpane", r"\.dpane\{[^}]*?padding:(\d+)px;[^}]*?gap:(\d+)px", [(1, S["detail-pane-padding"], "detail-pane-padding"), (2, S["pane-gap"], "pane-gap")])
literal_pairs(".dpane h3", r"\.dpane h3\{font-size:([\d.]+)px", [(1, F["size-detail-title"], "size-detail-title")])
literal_pairs("平台那一段的 .col", r'class="col" style="gap:(\d+)px">\s*<span class="sec">平台', [(1, S["section-gap"], "section-gap")])
literal_pairs(".facet", r"\.facet\{[^}]*?gap:(\d+)px", [(1, S["facet-gap"], "facet-gap")])
literal_pairs(".fchip", r"\.fchip\{[^}]*?gap:(\d+)px;height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:(\d+)px", [(1, S["facet-chip-gap"], "facet-chip-gap"), (2, L["facet-chip-height"], "facet-chip-height"), (3, L["facet-chip-padding"], "facet-chip-padding"), (4, F["size-small"], "size-small")])
# 挑选栏上那根容量条（票 `gui-looks-like-the-design/23`）：稿上 `.pickbar .gauge` 是
# `width:140px;height:8px`。**新令牌当场进这份名单**——`Q1083` 那个「绿着但没在看」的洞
# 就是这么一点点填上的。
literal_pairs(".pickbar .gauge", r"\.pickbar \.gauge\{width:(\d+)px;height:(\d+)px", [(1, L["pick-gauge-width"], "pick-gauge-width"), (2, L["pick-gauge-height"], "pick-gauge-height")])

# 分段开关（`.seg`）：票 `gui-looks-like-the-design/13` 把浏览屏那三组开关改用 `look::segmented`
# 时，这三个令牌是**手工比着稿核过一遍**的——那正是这份脚本该替人干的活。`Q1083`
# 那个「绿着但没在看」的洞，这一处就此堵上。
literal_pairs(".seg", r"\.seg\{[^}]*?padding:(\d+)px", [(1, L["seg-padding"], "seg-padding")])
literal_pairs(".seg button", r"\.seg button\{height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:(\d+)px", [(1, L["seg-button-height"], "seg-button-height"), (2, L["seg-button-padding"], "seg-button-padding"), (3, F["size-small"], "size-small")])
literal_pairs(".fchip small", r"\.fchip small\{[^}]*?font-size:([\d.]+)px", [(1, F["size-mini"], "size-mini")])
literal_pairs(".tag", r"\.tag\{[^}]*?height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:([\d.]+)px", [(1, L["tag-height"], "tag-height"), (2, L["tag-padding"], "tag-padding"), (3, F["size-caption-plus"], "size-caption-plus")])
literal_pairs(".note", r"\.note\{padding:(\d+)px (\d+)px;[^}]*?font-size:([\d.]+)px", [(1, S["note-padding"][0], "note-padding 上下"), (2, S["note-padding"][1], "note-padding 左右"), (3, F["size-small-plus"], "size-small-plus")])
literal_pairs(".opt", r"\.opt\{[^}]*?font-size:([\d.]+)px", [(1, F["size-small-plus"], "size-small-plus")])
literal_pairs(".opt small", r"\.opt small\{[^}]*?font-size:([\d.]+)px", [(1, F["size-caption-plus"], "size-caption-plus")])
literal_pairs(".var", r"\.var\{[^}]*?padding:(\d+)px (\d+)px;[^}]*?gap:(\d+)px (\d+)px", [(1, S["variant-card-padding"][0], "variant-card-padding 上下"), (2, S["variant-card-padding"][1], "variant-card-padding 左右"), (3, S["variant-card-gap"][0], "variant-card-gap 竖"), (4, S["variant-card-gap"][1], "variant-card-gap 横")])
literal_pairs(".var .p", r"\.var \.p\{[^}]*?font-size:(\d+)px", [(1, F["size-path"], "size-path")])
literal_pairs(".w2", r"\.w2\{[^}]*?font-size:(\d+)px", [(1, F["size-path"], "size-path")])
literal_pairs(".wcell", r"\.wcell\{[^}]*?gap:(\d+)px", [(1, S["cell-gap"], "cell-gap")])
literal_pairs(".tbl th", r"\.tbl th\{[^}]*?font-size:([\d.]+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, F["size-caption-plus"], "size-caption-plus"), (2, S["table-head-padding"][0], "table-head-padding 上下"), (3, S["table-head-padding"][1], "table-head-padding 左右")])
literal_pairs(".wtbl td", r"\.wtbl td\{padding:0 (\d+)px;height:(\d+)px", [(1, S["table-cell-padding"], "table-cell-padding"), (2, L["table-row"], "table-row")])
literal_pairs(".wtbl.lc td", r"\.wtbl\.lc td\{height:(\d+)px", [(1, L["table-row-cover"], "table-row-cover")])
literal_pairs(".wtbl .ck", r"\.wtbl \.ck\{width:(\d+)px;padding:0 0 0 (\d+)px", [(1, L["check-column"], "check-column"), (2, L["check-padding"], "check-padding")])
literal_pairs(".wtbl th .sa", r"\.wtbl th \.sa\{font-size:(\d+)px;margin-left:(\d+)px", [(1, F["size-arrow"], "size-arrow"), (2, S["sort-arrow-gap"], "sort-arrow-gap")])
literal_pairs("表头五列的宽", r'data-sort="t">作品</th><th style="width:(\d+)px"[^>]*>平台</th><th class="r" style="width:(\d+)px"[^>]*>变体</th><th class="r" style="width:(\d+)px"[^>]*>容量</th><th style="width:(\d+)px"[^>]*>年份</th><th style="width:(\d+)px">元数据', [(i + 1, L["table-columns"][i], f"table-columns[{i}]") for i in range(5)])
literal_pairs(".cbar", r"\.cbar\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, S["list-bar-gap"], "list-bar-gap"), (2, S["list-bar-padding"][0], "list-bar-padding 上下"), (3, S["list-bar-padding"][1], "list-bar-padding 左右")])
literal_pairs(".empty", r"\.empty\{padding:(\d+)px", [(1, S["empty-padding"], "empty-padding")])
literal_pairs(".lthumb", r"\.lthumb\{width:(\d+)px;height:(\d+)px;border-radius:(\d+)px;[^}]*?(\d+%)", [(1, L["thumb-list"][0], "thumb-list 宽"), (2, L["thumb-list"][1], "thumb-list 高"), (3, tokens["radius"]["small"], "radius small"), (4, M["thumb-list-tint"], "thumb-list-tint")])
literal_pairs(".lthumb i", r"\.lthumb i\{[^}]*?font-size:(\d+)px;[^}]*?inset 0 (\d+)px", [(1, F["size-thumb-code"], "size-thumb-code"), (2, L["thumb-list-band"], "thumb-list-band")])
literal_pairs(".dhead", r"\.dhead\{[^}]*?grid-template-columns:(\d+)px[^;]*;gap:(\d+)px", [(1, L["detail-cover-width"], "detail-cover-width"), (2, S["detail-head-gap"], "detail-head-gap")])
literal_pairs(".dcover", r"\.dcover\{[^}]*?border-radius:(\d+)px", [(1, tokens["radius"]["medium"], "radius medium")])
literal_pairs(".dcover .tcard", r"\.dcover \.tcard\{padding:(\d+)px (\d+)px", [(1, S["title-card-padding"][0], "title-card-padding 上下"), (2, S["title-card-padding"][1], "title-card-padding 左右")])
literal_pairs(".dcover .tc-t", r"\.dcover \.tc-t\{font-size:(\d+)px", [(1, F["size-cover-title"], "size-cover-title")])
literal_pairs(".dcover .tc-wm", r"\.dcover \.tc-wm\{font-size:(\d+)px;bottom:-(\d+)px", [(1, F["size-cover-mark"], "size-cover-mark"), (2, L["title-card-mark-offset"][1], "title-card-mark-offset 下")])
literal_pairs(".tcard", r"\.tcard\{[^}]*?(\d+%)[^}]*?inset 0 (\d+)px", [(1, M["title-card-tint"], "title-card-tint"), (2, L["title-card-band"], "title-card-band")])
literal_pairs(".tc-wm", r"\.tc-wm\{[^}]*?right:-(\d+)px;[^}]*?opacity:([\d.]+)", [(1, L["title-card-mark-offset"][0], "title-card-mark-offset 右"), (2, M["watermark-opacity"], "watermark-opacity")])
literal_pairs(".thumbs", r"\.thumbs\{[^}]*?repeat\((\d+),1fr\);gap:(\d+)px", [(1, L["thumbs-per-row"], "thumbs-per-row"), (2, S["thumb-gap"], "thumb-gap")])
literal_pairs(".gtree", r"\.gtree\{border:[^}]*?padding:(\d+)px", [(1, S["rule-box-padding"], "rule-box-padding")])
literal_pairs(".ruletext", r"\.ruletext\{[^}]*?font-size:(\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, F["size-path"], "size-path"), (2, S["rule-text-padding"][0], "rule-text-padding 上下"), (3, S["rule-text-padding"][1], "rule-text-padding 左右")])
literal_pairs(".iconbtn", r"\.iconbtn\{width:(\d+)px;height:(\d+)px", [(1, L["icon-button"], "icon-button 宽"), (2, L["icon-button"], "icon-button 高")])
literal_pairs(".strip", r"\.strip\{[^}]*?gap:(\d+)px;padding:(\d+)px 0", [(1, S["strip-gap"], "strip-gap"), (2, S["strip-padding"], "strip-padding")])

# 待确认屏「细分」那一栏底下那条占比条，与下钻之后就地那一框（票 gui-looks-like-the-design/19）。
literal_pairs(".dist .b", r"\.dist \.b\{[^}]*?height:(\d+)px", [(1, L["dist-bar"], "dist-bar")])
literal_pairs(".dist .b i", r"\.dist \.b i\{[^}]*?opacity:([\d.]+)", [(1, M["dist-bar-opacity"], "dist-bar-opacity")])
literal_pairs(".drill", r"\.drill\{[^}]*?var\(--accent\) (\d+%),[^}]*?padding:(\d+)px (\d+)px;[^}]*?gap:(\d+)px", [(1, M["drill-tint"], "drill-tint"), (2, S["drill-padding"][0], "drill-padding 上下"), (3, S["drill-padding"][1], "drill-padding 左右"), (4, S["drill-gap"], "drill-gap")])

# 疑似同一作品那张建议卡（票 gui-looks-like-the-design/17）。
literal_pairs(".sugg", r"\.sugg\{[^}]*?var\(--accent\) (\d+%),[^}]*?padding:(\d+)px (\d+)px;[^}]*?gap:(\d+)px", [(1, M["suspicion-tint"], "suspicion-tint"), (2, L["suspicion-padding"][0], "suspicion-padding 上下"), (3, L["suspicion-padding"][1], "suspicion-padding 左右"), (4, L["suspicion-gap"], "suspicion-gap")])
literal_pairs(".sugg ul", r"\.sugg ul\{[^}]*?gap:(\d+)px", [(1, L["suspicion-reason-gap"], "suspicion-reason-gap")])

# 平台那一簇先摆几个：「更多（N）」前头没带 data-more 的那几枚。
plats = re.search(r'<span class="sec">平台</span>\s*<div class="facet">(.*?)id="more-plat"', html, re.S)
if not plats:
    problems.append("找不到平台那一簇")
else:
    literals += 1
    shown = len(re.findall(r'data-plat="[^"]+" aria-pressed', plats[1]))
    if shown != L["platforms-visible"]:
        problems.append(f"platforms-visible: 设计稿先摆 {shown} 个，令牌是 {L['platforms-visible']}")

# 库屏（设计稿 .libgrid / .phead / .stage / .nextline / .tbl）：两栏间距、面板标题栏、工序那一行的列宽、列缝、
# 内边距、圆点、字号、状态竖条与图标按钮。都是字面值，照字面值核。
def literal(pattern: str, what: str):
    found = re.search(pattern, html)
    if not found:
        problems.append(f"找不到 {what}")
    return found


def same(got: str, want, key: str):
    global literals
    literals += 1
    if float(got) != float(want):
        problems.append(f"{key}: 设计稿是 {got}px，令牌是 {want}")


def in_steps(got: str, what: str):
    """稿上这个数落在不落在间距档位里。核的是稿，不是那几档：读不记账的原始令牌，档位本身另进豁免表。"""
    global literals
    literals += 1
    steps = raw_tokens["space"]["steps"]
    if float(got) not in [float(s) for s in steps]:
        problems.append(f"{what} {got}px 不在间距档位 {steps} 里")


panel_padding = tokens["space"]["panel-padding"]
if m := literal(r"\.libgrid\{[^}]*?gap:(\d+)px", ".libgrid 的 gap"):
    same(m[1], tokens["space"]["library-gap"], "library-gap")
if m := literal(r"\.phead\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", ".phead 的 gap 与 padding"):
    same(m[1], tokens["space"]["panel-head-gap"], "panel-head-gap")
    same(m[2], panel_padding[0], "panel-padding 上下（.phead）")
    same(m[3], panel_padding[1], "panel-padding 左右（.phead）")
if m := literal(r"\.phead h3\{font-size:([\d.]+)px", ".phead h3 的 font-size"):
    same(m[1], tokens["font"]["size-panel-title"], "size-panel-title")
if m := literal(r"\.stage\{[^}]*?grid-template-columns:(\d+)px (\d+)px 1fr auto;gap:(\d+)px;[^}]*?padding:(\d+)px (\d+)px", ".stage 的列宽、gap 与 padding"):
    same(m[1], tokens["layout"]["stage-columns"][0], "stage-columns 圆点那一列")
    same(m[2], tokens["layout"]["stage-columns"][1], "stage-columns 工序名那一列")
    in_steps(m[3], ".stage 的 gap")
    same(m[4], panel_padding[0], "panel-padding 上下（.stage）")
    same(m[5], panel_padding[1], "panel-padding 左右（.stage）")
if m := literal(r"\.stage \.dot\{width:(\d+)px;[^}]*?border:([\d.]+)px", ".stage .dot 的 width 与 border"):
    same(m[1], tokens["layout"]["stage-dot"], "stage-dot")
    same(m[2], tokens["layout"]["stage-dot-stroke"], "stage-dot-stroke")
if m := literal(r"\.stage \.left\{font-size:([\d.]+)px", ".stage .left 的 font-size"):
    same(m[1], tokens["font"]["size-small-plus"], "size-small-plus（.stage .left）")
if m := literal(r"\.stage \.left small\{[^}]*?font-size:([\d.]+)px", ".stage .left small 的 font-size"):
    same(m[1], tokens["font"]["size-caption-plus"], "size-caption-plus（.stage .left small）")
if m := literal(r"\.stage\.next\{[^}]*?inset (\d+)px", ".stage.next 的竖条"):
    same(m[1], tokens["layout"]["row-stripe"], "row-stripe（.stage.next）")
if m := literal(r"\.tbl td\.st\{box-shadow:inset (\d+)px", ".tbl td.st 的竖条"):
    same(m[1], tokens["layout"]["row-stripe"], "row-stripe（.tbl td.st）")
if m := literal(r"\.nextline\{[^}]*?gap:(\d+)px;[^}]*?padding:(\d+)px;", ".nextline 的 gap 与 padding"):
    in_steps(m[1], ".nextline 的 gap")
    same(m[2], panel_padding[1], "下一步那一块的内边距（panel-padding 左右）")
if m := literal(r'style="width:(\d+)px" aria-label="前端格式"', "导出设置那一块前端格式下拉的 width"):
    same(m[1], tokens["layout"]["format-select-width"], "format-select-width")

# 子库屏（票 gui-looks-like-the-design/20）：容量条的高与「未知」那一段斜纹一个来回、图例色块的边长与圆角、
# 规则行首序号圆的直径、空态卡的内边距——设计稿里都是字面值，照字面值核。
for pattern, key in [
    (r"\.gauge\{[^}]*?height:(\d+)px", "gauge-height"),
    (r"\.gauge \.unk\{[^}]*?transparent \d+px (\d+)px", "gauge-hatch"),
    (r"\.legend i\{[^}]*?width:(\d+)px", "legend-swatch"),
    (r"\.legend i\{[^}]*?border-radius:(\d+)px", "legend-swatch-radius"),
    (r"\.rule \.rn\{[^}]*?width:(\d+)px", "rule-badge"),
    (r'<div class="card" style="padding:(\d+)px;text-align:center;max-width:620px', "empty-card-padding"),
]:
    m = re.search(pattern, html)
    if not m:
        problems.append(f"找不到 {key} 在设计稿里的那个字面值")
        continue
    literals += 1
    if int(m[1]) != tokens["layout"][key]:
        problems.append(f"{key}: 设计稿是 {m[1]}px，令牌是 {tokens['layout'][key]}")

# 提示条（设计稿 .toast，票 gui-looks-like-the-design/20 删除子库之后那一条）与弹层里「会怎样」那几条（.impact）：
# 离底边多远、四边留白、按钮描边多淡、停多久、行首圆点多大、那一列多宽——设计稿里都是字面值。
m = re.search(r"\.toast\{[^}]*?bottom:(\d+)px;[^}]*?padding:(\d+)px (\d+)px (\d+)px (\d+)px", html)
if not m:
    problems.append("找不到 .toast 的 bottom 与 padding")
else:
    literals += 1
    if int(m[1]) != tokens["layout"]["toast-bottom"]:
        problems.append(f"toast-bottom: 设计稿是 {m[1]}px，令牌是 {tokens['layout']['toast-bottom']}")
    literals += 1
    got = [int(m[i]) for i in range(2, 6)]
    if got != tokens["layout"]["toast-padding"]:
        problems.append(f"toast-padding: 设计稿是 {got}，令牌是 {tokens['layout']['toast-padding']}")
m = re.search(r"\.toast \.btn\{[^}]*?rgba\(255,255,255,(\.?\d+)\)", html)
if not m:
    problems.append("找不到 .toast .btn 的描边")
else:
    literals += 1
    if float(m[1]) != float(tokens["layout"]["toast-button-line"]):
        problems.append(f"toast-button-line: 设计稿是 {m[1]}，令牌是 {tokens['layout']['toast-button-line']}")
m = re.search(r"toastT=setTimeout\(.*?act\?(\d+):(\d+)\)", html)
if not m:
    problems.append("找不到 toast() 停多久的那两个毫秒数")
else:
    for got, key in [(m[1], "toast-action-seconds"), (m[2], "toast-seconds")]:
        literals += 1
        if int(got) != round(float(tokens["layout"][key]) * 1000):
            problems.append(f"{key}: 设计稿是 {got} 毫秒，令牌是 {tokens['layout'][key]} 秒")
for pattern, key in [
    (r"\.impact li::before\{[^}]*?width:(\d+)px", "impact-dot"),
    (r"\.impact li\{[^}]*?grid-template-columns:(\d+)px", "impact-column"),
]:
    m = re.search(pattern, html)
    if not m:
        problems.append(f"找不到 {key} 在设计稿里的那个字面值")
        continue
    literals += 1
    if int(m[1]) != tokens["layout"][key]:
        problems.append(f"{key}: 设计稿是 {m[1]}px，令牌是 {tokens['layout'][key]}")

# 规则行尾那颗图标按钮（设计稿 .iconbtn）：字号。边长 icon-button 在上面浏览屏那一段与高一起核过，这儿不再核。
m = re.search(r"\.iconbtn\{width:(\d+)px;[^}]*?font-size:(\d+)px", html)
if not m:
    problems.append("找不到 .iconbtn 的 width 与 font-size")
else:
    for got, section, key in [(m[2], "font", "size-body")]:
        literals += 1
        if int(got) != tokens[section][key]:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {tokens[section][key]}")

# 两档半号字号在设计稿 .tbl th（11.5px）、.note（12.5px）上的那两处，上面浏览屏那一段已经核过（同一个键），这儿不再核。

# 子库屏空态那张卡的标题字号：设计稿 renderDevs 里写在 h3 上的字面值。
m = re.search(r'<h3 style="font-size:(\d+)px">还没有子库</h3>', html)
if not m:
    problems.append("找不到子库屏空态卡标题的 font-size")
else:
    literals += 1
    if int(m[1]) != tokens["font"]["size-empty-title"]:
        problems.append(f"size-empty-title: 设计稿是 {m[1]}px，令牌是 {tokens['font']['size-empty-title']}")

# 底部状态栏里任务那条小进度条（设计稿 .statusbar .mini .bar）：宽。
m = re.search(r"\.statusbar \.mini \.bar\{[^}]*?width:(\d+)px", html)
if not m:
    problems.append("找不到 .statusbar .mini .bar 的 width")
else:
    literals += 1
    if int(m[1]) != tokens["layout"]["statusbar-bar"]:
        problems.append(f"statusbar-bar: 设计稿是 {m[1]}px，令牌是 {tokens['layout']['statusbar-bar']}")

# 正在跑那张卡左边那条强调色竖条（设计稿 .runcard 的 box-shadow:inset 3px 0 0 var(--accent)）：宽。
m = re.search(r"\.runcard\{[^}]*?box-shadow:inset (\d+)px 0 0 var\(--accent\)", html)
if not m:
    problems.append("找不到 .runcard 的 box-shadow")
else:
    literals += 1
    if int(m[1]) != tokens["layout"]["runcard-bar"]:
        problems.append(f"runcard-bar: 设计稿是 {m[1]}px，令牌是 {tokens['layout']['runcard-bar']}")

# 任务屏（票 gui-looks-like-the-design/25）：块与块、卡片、表格、空态的留白（屏头、屏体的内边距归下面外壳那一段核；
# 空态 .empty 与表头 .tbl th 的留白与浏览屏同一个令牌，在上面浏览屏那一段核过，这儿不再核一遍）。
space, layout = tokens["space"], tokens["layout"]
for pattern, wants in [
    (r'id="s-task".*?class="scrbody col" style="gap:(\d+)px"', [("screen-section-gap", space["screen-section-gap"])]),
    (r'id="s-task".*?<div class="col" style="gap:(\d+)px"><span class="sec">正在运行', [("section-title-gap", space["section-title-gap"])]),
    (r"\.runcard\{padding:(\d+)px;[^}]*?gap:(\d+)px \d+px", [("card-padding", space["card-padding"]), ("card-row-gap", space["card-row-gap"])]),
    (r"\.runcard \.bar\{[^}]*?height:(\d+)px", [("runcard-progress", layout["runcard-progress"])]),
    (r"\.runcard \.meta\{[^}]*?gap:(\d+)px", [("meta-gap", space["meta-gap"])]),
    (r"\$\('#tqueue'\).*?class=\"card row\" style=\"padding:(\d+)px (\d+)px\"", [("queue-row-padding 上下", space["queue-row-padding"][0]), ("queue-row-padding 左右", space["queue-row-padding"][1])]),
    (r"\.tbl td\{[^}]*?padding:(\d+)px (\d+)px", [("cell-padding 上下", space["cell-padding"][0]), ("cell-padding 左右", space["cell-padding"][1])]),
]:
    m = re.search(pattern, html, re.S)
    if not m:
        problems.append(f"找不到 {pattern}")
        continue
    for got, (key, want) in zip(m.groups(), wants):
        literals += 1
        if int(got) != want:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {want}")

# 两档半号字号（拿主意的人裁，挂单 Q840 / Q862）：设计稿 .scrhead .sub、.hist td 的 12.5px，.railfoot .st 的 11.5px
# （.tbl th 的 11.5px 在上面浏览屏那一段与表头内边距一起核）。
for selector, key in [(r"\.scrhead \.sub\{", "size-small-plus"), (r"\.hist td\{", "size-small-plus"), (r"\.railfoot \.st\{", "size-caption-plus")]:
    m = re.search(selector + r"[^}]*?font-size:([\d.]+)px", html)
    if not m:
        problems.append(f"找不到 {selector} 的 font-size")
        continue
    literals += 1
    if float(m[1]) != tokens["font"][key]:
        problems.append(f"{key}: 设计稿是 {m[1]}px，令牌是 {tokens['font'][key]}")

# 屏头与屏体（设计稿 .scrhead / .scrbody）：内边距与间距。
m = re.search(r"\.scrhead\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", html)
if not m:
    problems.append("找不到 .scrhead 的 gap 与 padding")
else:
    header_padding = tokens["space"]["screen-header-padding"]
    for got, want, key in [(m[1], tokens["space"]["screen-header-gap"], "screen-header-gap"), (m[2], header_padding[0], "screen-header-padding 上下"), (m[3], header_padding[1], "screen-header-padding 左右")]:
        literals += 1
        if int(got) != want:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {want}")
m = re.search(r"\.scrbody\{[^}]*?padding:(\d+)px (\d+)px (\d+)px", html)
if not m:
    problems.append("找不到 .scrbody 的 padding")
else:
    for got, want, key in zip(m.groups(), tokens["space"]["screen-body-padding"], ["screen-body-padding 上", "screen-body-padding 左右", "screen-body-padding 下"]):
        literals += 1
        if int(got) != want:
            problems.append(f"{key}: 设计稿是 {got}px，令牌是 {want}")

# 左栏（设计稿 .rail / .libsw / .grp / .nav / .railfoot，以及收成窄条的 .main.rcol 那几条）：写的是字面值，照字面值核。
# 每条：(正则, [(第几组, 令牌节, 令牌键, 取数组第几格或 None)])。
def token(section, key, at):
    value = tokens[section][key]
    return value if at is None else value[at]


def check_literals(entries):
    """逐条核对设计稿里写死的字面值；交回核了几项。带 % 的按成数比（设计稿 7% 对令牌 0.07）。"""
    checked = 0
    for pattern, checks in entries:
        m = re.search(pattern, html)
        if not m:
            problems.append(f"找不到 {pattern}")
            continue
        for group, section, key, at in checks:
            checked += 1
            want = token(section, key, at)
            got = m[group]
            if abs(literal_value(got) - float(want)) > 1e-9:
                where = key if at is None else f"{key}[{at}]"
                problems.append(f"{where}: 设计稿是 {got}，令牌是 {want}")
    return checked

rail_literals = [
    (r"\.rail\{[^}]*?padding:(\d+)px (\d+)px;gap:(\d+)px", [(1, "space", "rail-padding", 0), (2, "space", "rail-padding", 1), (3, "space", "rail-gap", None)]),
    (r"\.main\.rcol \.rail\{padding:(\d+)px (\d+)px", [(1, "space", "rail-padding-collapsed", 0), (2, "space", "rail-padding-collapsed", 1)]),
    (r"\.libsw\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px;margin-bottom:(\d+)px", [(1, "space", "rail-switch-gap", None), (2, "space", "rail-switch-padding", 0), (3, "space", "rail-switch-padding", 1), (4, "space", "rail-switch-margin", None)]),
    (r"\.libsw \.mark\{width:(\d+)px", [(1, "layout", "rail-mark", None)]),
    (r"\.grp\{[^}]*?padding:(\d+)px (\d+)px (\d+)px;letter-spacing:([\d.]+)em", [(1, "space", "rail-group-padding", 0), (2, "space", "rail-group-padding", 1), (3, "space", "rail-group-padding", 2), (4, "font", "group-tracking", None)]),
    (r"\.main\.rcol \.grp\{[^}]*?margin:(\d+)px (\d+)px", [(1, "space", "rail-group-margin-collapsed", 0), (2, "space", "rail-group-margin-collapsed", 1)]),
    (r"\.nav\{[^}]*?height:(\d+)px;padding:0 (\d+)px", [(1, "layout", "nav-height", None), (2, "space", "nav-padding", None)]),
    (r"\.main\.rcol \.nav\{[^}]*?padding:(\d+)px 0;gap:(\d+)px", [(1, "space", "nav-padding-collapsed", None), (2, "space", "nav-gap-collapsed", None)]),
    (r"\.main\.rcol \.nav \.badge\{[^}]*?font-size:(\d+)px", [(1, "font", "size-badge-narrow", None)]),
    (r"\.nav \.badge\.live\{[^}]*?gap:(\d+)px", [(1, "space", "live-badge-gap", None)]),
    (r"\.nav \.badge\.live::before\{[^}]*?width:(\d+)px", [(1, "layout", "rail-dot", None)]),
    (r"\.railfoot\{[^}]*?gap:(\d+)px;padding-top:(\d+)px", [(1, "space", "rail-foot-gap", None), (2, "space", "rail-foot-padding", None)]),
    (r"\.railfoot \.st\{[^}]*?padding:0 (\d+)px;[^}]*?gap:(\d+)px", [(1, "space", "rail-note-padding", None), (2, "space", "rail-note-gap", None)]),
    (r"\.railfoot \.st::before\{[^}]*?width:(\d+)px", [(1, "layout", "rail-dot", None)]),
]
literals += check_literals(rail_literals)

# 标签与按钮的字号（设计稿 .chip / .btn / .btn.sm / .btn.lg），以及标签的高、左右留白、圆点与字的间距。
shared_literals = [
    (r"\.chip\{[^}]*?gap:(\d+)px;height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:([\d.]+)px", [(1, "layout", "chip-gap", None), (2, "layout", "chip-height", None), (3, "layout", "chip-padding", None), (4, "font", "size-caption-plus", None)]),
    (r"\.ro\{[^}]*?gap:(\d+)px;height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:(\d+)px", [(1, "layout", "ro-gap", None), (2, "layout", "ro-height", None), (3, "layout", "ro-padding", None), (4, "font", "size-small", None)]),
    (r"\.ro::before\{[^}]*?width:(\d+)px", [(1, "layout", "chip-dot", None)]),
    (r"\.btn\{[^}]*?font-size:([\d.]+)px", [(1, "font", "size-small-plus", None)]),
    (r"\.btn\.sm\{[^}]*?font-size:(\d+)px", [(1, "font", "size-small", None)]),
    (r"\.btn\.lg\{[^}]*?font-size:(\d+)px", [(1, "font", "size-button-large", None)]),
]
literals += check_literals(shared_literals)

# 设置屏（票 gui-looks-like-the-design/31）：.sets / .srow / .switch / .kgrid 那几处字面量。
settings_literals = [
    (r"\.sets\{[^}]*?grid-template-columns:(\d+)px[^}]*?gap:(\d+)px", [(1, "layout", "settings-nav-width", None), (2, "space", "settings-gap", None)]),
    (r"\.sets nav\{[^}]*?gap:(\d+)px;border-right:1px solid var\(--line\);padding-right:(\d+)px", [(1, "space", "settings-nav-gap", None), (2, "space", "settings-nav-divider", None)]),
    (r"\.sets nav button\{height:(\d+)px;padding:0 (\d+)px", [(1, "layout", "settings-nav-height", None), (2, "space", "settings-nav-padding", None)]),
    (r"\.sset\{[^}]*?gap:(\d+)px", [(1, "space", "settings-body-gap", None)]),
    (r"\.srow\{[^}]*?grid-template-columns:(\d+)px[^}]*?gap:(\d+)px (\d+)px;[^}]*?padding-bottom:(\d+)px", [(1, "layout", "settings-row-label", None), (2, "space", "settings-row-gap", 0), (3, "space", "settings-row-gap", 1), (4, "space", "settings-row-bottom", None)]),
    (r"\.srow>b\{[^}]*?padding-top:(\d+)px", [(1, "space", "settings-label-top", None)]),
    (r"\.switch\{[^}]*?gap:(\d+)px", [(1, "space", "settings-switch-gap", None)]),
    (r"\.switch i\{width:(\d+)px;height:(\d+)px", [(1, "layout", "settings-switch", 0), (2, "layout", "settings-switch", 1)]),
    (r"\.switch i::after\{[^}]*?width:(\d+)px", [(1, "layout", "settings-switch", 2)]),
    (r"\.kgrid\{[^}]*?gap:(\d+)px (\d+)px", [(1, "space", "keys-grid-gap", 0), (2, "space", "keys-grid-gap", 1)]),
    (r"\.kgrid div\{[^}]*?gap:(\d+)px;padding:(\d+)px 0", [(1, "space", "keys-row-gap", None), (2, "space", "keys-row-padding", None)]),
    # 快捷键表两组之间：设置屏那一节由 .sset 给，按 ? 那层弹层由 .mbody 给——两处都是 14。
    (r"\.mbody\{[^}]*?gap:(\d+)px", [(1, "space", "keys-group-gap", None)]),
]
literals += check_literals(settings_literals)

# 右键菜单（票 gui-looks-like-the-design/14）：设计稿 .ctx 那一簇。
# 新立一个令牌就往这儿补一条——「绿着没人看」比红了更危险（挂单 Q1083）。
menu_literals = [
    (r"\.ctx\{[^}]*?min-width:(\d+)px;padding:(\d+)px", [(1, "layout", "menu-min-width", None), (2, "space", "menu-padding", None)]),
    (r"\.ctx button\{[^}]*?gap:(\d+)px;width:100%;height:(\d+)px;padding:0 (\d+)px;[^}]*?font-size:([\d.]+)px", [(1, "space", "menu-item-gap", None), (2, "layout", "menu-item-height", None), (3, "space", "menu-item-padding", None), (4, "font", "size-small-plus", None)]),
    (r"\.ctx button span\{[^}]*?font-size:(\d+)px", [(1, "font", "size-caption", None)]),
    (r"\.ctx hr\{[^}]*?margin:(\d+)px (\d+)px", [(1, "space", "menu-rule-margin", 0), (2, "space", "menu-rule-margin", 1)]),
    (r"\.ctx \.hd\{padding:(\d+)px (\d+)px (\d+)px;font-size:([\d.]+)px;[^}]*?max-width:(\d+)px", [(1, "space", "menu-head-padding", 0), (2, "space", "menu-head-padding", 1), (3, "space", "menu-head-padding", 2), (4, "font", "size-caption-plus", None), (5, "layout", "menu-head-width", None)]),
]
literals += check_literals(menu_literals)

# ── 票 gate-and-tests/07 补齐的存量 ──
# 下面这些令牌此前一条核对都没有，脚本照样报「一致」（挂单 Q963 / Q1083）。完整性检查立起来之后，
# 它印出来的漏核清单就是这一批：设计稿里有对应物的都在这儿写了核对，没有的进文件末尾的豁免表。

# 外壳：左栏两档宽与状态栏高（.main 的两列两行）、浏览屏左右两栏的宽与收起后的窄条、控件描边、字体。
shell_literals = [
    (r"\.main\{[^}]*?grid-template-columns:(\d+)px 1fr;grid-template-rows:1fr (\d+)px", [(1, "layout", "rail-width", None), (2, "layout", "statusbar", None)]),
    (r"\.main\.rcol\{grid-template-columns:(\d+)px 1fr", [(1, "layout", "rail-collapsed", None)]),
    (r"\.browse\{--fw:(\d+)px;--dw:(\d+)px", [(1, "layout", "filter-pane-width", None), (2, "layout", "detail-pane-width", None)]),
    (r"\.browse\.fcol\{--fw:(\d+)px\}\.browse\.dcol\{--dw:(\d+)px\}", [(1, "layout", "strip-width", None), (2, "layout", "strip-width", None)]),
    (r"\.btn\{[^}]*?border:(\d+)px solid", [(1, "layout", "control-stroke", None)]),
    (r"\.input\{[^}]*?border:(\d+)px solid", [(1, "layout", "control-stroke", None)]),
    (r"\.btn\[disabled\]\{opacity:([\d.]+)", [(1, "mix", "disabled-opacity", None)]),
    (r"body\{[^}]*?font:(\d+)px/([\d.]+) var\(--sans\)", [(1, "font", "size-body", None), (2, "font", "line-height", None)]),
    (r"h1,h2,h3,h4\{[^}]*?font-weight:(\d+)", [(1, "font", "weight-strong", None)]),
    (r"\.scrhead h2\{font-size:(\d+)px", [(1, "font", "size-page", None)]),
    # size-title 的注释写的是「卡片标题、对话框标题」：卡片（.runcard h3 / .dev h3）与弹层标头（.mhead h3）三处都核。
    (r"\.runcard h3\{font-size:(\d+)px", [(1, "font", "size-title", None)]),
    (r"\.dev h3\{font-size:(\d+)px", [(1, "font", "size-title", None)]),
    (r"\.mhead h3\{font-size:(\d+)px", [(1, "font", "size-title", None)]),
]
literals += check_literals(shell_literals)

# 开场与弹层（.opening / .op-hero / .op-side / .mark / .promise / .step / .mbody / .kv / .frm / .warnbox）。
opening_literals = [
    (r"\.opening\{[^}]*?minmax\((\d+)px,1fr\) minmax\((\d+)px,(\d+)px\)", [(1, "layout", "opening-hero-min", None), (2, "layout", "opening-side-width", 0), (3, "layout", "opening-side-width", 1)]),
    (r"\.op-hero\{padding:(\d+)px (\d+)px", [(1, "space", "opening-hero-padding", 0), (2, "space", "opening-hero-padding", 1)]),
    (r"\.op-side\{padding:(\d+)px (\d+)px", [(1, "space", "opening-side-padding", 0), (2, "space", "opening-side-padding", 1)]),
    (r"(?m)^\.mark\{width:(\d+)px;height:(\d+)px", [(1, "layout", "mark", None), (2, "layout", "mark", None)]),
    (r"\.promise \.ic\{width:(\d+)px;height:(\d+)px", [(1, "layout", "promise-icon", None), (2, "layout", "promise-icon", None)]),
    (r"\.step i\{width:(\d+)px;height:(\d+)px", [(1, "layout", "page-dot", None), (2, "layout", "page-dot", None)]),
    (r"\.mbody\{padding:(\d+)px (\d+)px", [(1, "space", "dialog-padding", 0), (2, "space", "dialog-padding", 1)]),
    # 弹层只用四档宽：设计稿说明表里那一句写着这四个数。
    (r"宽度只用 (\d+) / (\d+) / (\d+) / (\d+) 四档", [(i + 1, "layout", "dialog-width", i) for i in range(4)]),
    (r"\.kv\{[^}]*?grid-template-columns:(\d+)px", [(1, "layout", "kv-key-width", None)]),
    (r"\.frm\{[^}]*?grid-template-columns:(\d+)px", [(1, "layout", "form-label-width", None)]),
    (r"\.warnbox\{padding:(\d+)px (\d+)px", [(1, "layout", "warn-box-padding", 0), (2, "layout", "warn-box-padding", 1)]),
    (r"\.opt\{[^}]*?gap:(\d+)px;[^}]*?padding:(\d+)px 0", [(1, "layout", "option-gap", None), (2, "layout", "option-padding", None)]),
]
literals += check_literals(opening_literals)
# 那一句是稿自己的声明；稿里弹层实际用到的宽也得都落在这四档里：DLG 各页交回的 w:、渲染时的缺省 .w||620、
# .modal 的缺省宽、直接写在弹层上的 style 宽。
used_widths = {int(w) for w in re.findall(r"[{,]w:(\d+)[,}]", html) + re.findall(r"\.w\|\|(\d+)", html)
               + re.findall(r"\.modal\{width:(\d+)px", html) + re.findall(r'class="modal"[^>]*?style="width:(\d+)px', html)}
literals += 1
if not used_widths:
    problems.append("找不到设计稿里弹层用到的宽")
elif off_tiers := sorted(used_widths - {int(w) for w in L["dialog-width"]}):
    problems.append(f"dialog-width: 设计稿的弹层用到了 {off_tiers}，不在令牌那几档 {list(L['dialog-width'])} 里")

# 浏览屏的卡片视图（.cgrid / .cover / .cv-tier）：三档封面宽、封面宽高比、封面底下那道置信度色带。
card_literals = [
    (r"\.cgrid\{--cw:(\d+)px", [(1, "layout", "card-widths", 1)]),
    (r"\.cgrid\.s\{--cw:(\d+)px\}\.cgrid\.l\{--cw:(\d+)px\}", [(1, "layout", "card-widths", 0), (2, "layout", "card-widths", 2)]),
    (r"\.cv-tier\{[^}]*?height:(\d+)px", [(1, "layout", "tier-bar", None)]),
]
literals += check_literals(card_literals)
m = re.search(r"\.cover\{[^}]*?aspect-ratio:(\d+)/(\d+)", html)
if not m:
    problems.append("找不到 .cover 的 aspect-ratio")
else:
    literals += 1
    if abs(int(m[1]) / int(m[2]) - L["card-cover-ratio"]) > 1e-9:
        problems.append(f"card-cover-ratio: 设计稿是 {m[1]}/{m[2]}，令牌是 {L['card-cover-ratio']}")

# 待确认屏（票 gui-looks-like-the-design/18、19）：正文头上三格、空态卡、一批变体卡、逐条那一屏、候选卡、键位提示、
# 中文离线源那一堆（挂单 Q963 点名的那一批）。
queue_literals = [
    (r"\.qsum\{[^}]*?gap:(\d+)px;margin-bottom:(\d+)px", [(1, "space", "queue-summary-gap", None), (2, "space", "queue-summary-margin", None)]),
    (r"\.qcell\{padding:(\d+)px (\d+)px", [(1, "space", "queue-cell-padding", 0), (2, "space", "queue-cell-padding", 1)]),
    (r"\.qcell \.v\{[^}]*?font-size:(\d+)px", [(1, "font", "size-summary-count", None)]),
    (r'id="q-empty"[^>]*>\s*<div class="card" style="max-width:(\d+)px;margin:(\d+)px auto;padding:(\d+)px', [(1, "layout", "empty-state-width", None), (2, "space", "empty-state-margin", None), (3, "space", "empty-state-padding", None)]),
    (r'(?s)id="q-empty".*?<p class="dim" style="margin:(\d+)px 0 (\d+)px">.*?<p class="help" style="margin-top:(\d+)px">', [(1, "space", "empty-state-gaps", 0), (2, "space", "empty-state-gaps", 1), (3, "space", "empty-state-gaps", 2)]),
    (r"\.batch\{[^}]*?margin-bottom:(\d+)px;[^}]*?box-shadow:inset (\d+)px 0 0 var\(--c\)", [(1, "space", "batch-gap", None), (2, "layout", "tier-bar", None)]),
    (r"\.bhead\{[^}]*?grid-template-columns:(\d+)px 1fr auto (\d+)px;gap:(\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, "layout", "batch-count-width", None), (2, "layout", "batch-chevron-column", None), (3, "space", "batch-head-gap", None), (4, "space", "batch-head-padding", 0), (5, "space", "batch-head-padding", 1)]),
    (r"\.bhead \.cnt\{[^}]*?font-size:(\d+)px", [(1, "font", "size-batch-count", None)]),
    (r"\.bhead \.why1\{[^}]*?margin-top:(\d+)px", [(1, "space", "batch-line-gap", None)]),
    (r"\.shape\{[^}]*?gap:(\d+)px (\d+)px", [(1, "space", "shape-gap", 0), (2, "space", "shape-gap", 1)]),
    (r"\.chev\{width:(\d+)px;height:(\d+)px;border-right:([\d.]+)px", [(1, "layout", "chevron", None), (2, "layout", "chevron", None), (3, "layout", "chevron-stroke", None)]),
    (r"\.bbody\{padding:(\d+)px (\d+)px (\d+)px;[^}]*?grid-template-columns:minmax\(0,([\d.]+)fr\) minmax\(0,([\d.]+)fr\);gap:(\d+)px", [(1, "space", "batch-body-padding", 0), (2, "space", "batch-body-padding", 1), (3, "space", "batch-body-padding", 2), (4, "layout", "batch-body-columns", 0), (5, "layout", "batch-body-columns", 1), (6, "space", "batch-body-gap", None)]),
    (r"\.why\{[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "why-padding", 0), (2, "space", "why-padding", 1)]),
    (r"\.dist\{[^}]*?gap:(\d+)px \d+px", [(1, "space", "dist-row-gap", None)]),
    (r"\.smp\{[^}]*?grid-template-columns:minmax\(0,1fr\) (\d+)px minmax\(0,1fr\);[^}]*?padding:(\d+)px 0", [(1, "layout", "sample-arrow-column", None), (2, "space", "sample-row-padding", None)]),
    (r"\.bare\{margin-top:(\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "bare-margin", None), (2, "space", "bare-padding", 0), (3, "space", "bare-padding", 1)]),
    (r'(?s)<div class="bare">.*?<p class="help" style="margin-top:(\d+)px">', [(1, "space", "bare-note-gap", None)]),
    (r"\.lot\{padding:(\d+)px (\d+)px", [(1, "space", "record-row-padding", 0), (2, "space", "record-row-padding", 1)]),
    (r"\.drawer\{[^}]*?width:(\d+)px", [(1, "layout", "drawer-width", None)]),
    (r'<div class="obolist">\s*<div class="ptitle" style="padding:(\d+)px (\d+)px (\d+)px (\d+)px', [(i + 1, "space", "list-head-padding", i) for i in range(4)]),
    (r"\.oboit\{[^}]*?padding:(\d+)px (\d+)px (\d+)px (\d+)px", [(i + 1, "space", "list-item-padding", i) for i in range(4)]),
    (r"\.obodet\{[^}]*?padding:(\d+)px (\d+)px;[^}]*?gap:(\d+)px", [(1, "space", "detail-padding", 0), (2, "space", "detail-padding", 1), (3, "space", "detail-gap", None)]),
    (r"\$\('#obo-det'\)\.innerHTML=`<div class=\"col\" style=\"gap:(\d+)px\"", [(1, "space", "obo-head-gap", None)]),
    (r"\.cands\{[^}]*?gap:(\d+)px", [(1, "space", "candidate-gap", None)]),
    (r"\.cand\{[^}]*?padding:(\d+)px;[^}]*?gap:(\d+)px", [(1, "space", "candidate-padding", None), (2, "space", "candidate-inner-gap", None)]),
    (r'\.cand\[aria-selected="true"\]\{[^}]*?0 0 0 (\d+)px var\(--accent-soft\)', [(1, "layout", "candidate-ring", None)]),
    (r"\.cand \.t\{[^}]*?font-size:([\d.]+)px", [(1, "font", "size-candidate-title", None)]),
    (r"\.cand dl\{[^}]*?grid-template-columns:(\d+)px 1fr;gap:(\d+)px (\d+)px", [(1, "layout", "candidate-key-width", None), (2, "space", "candidate-row-gap", 0), (3, "space", "candidate-row-gap", 1)]),
    (r"\.keys\{[^}]*?gap:(\d+)px (\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "keys-gap", 0), (2, "space", "keys-gap", 1), (3, "space", "keys-padding", 0), (4, "space", "keys-padding", 1)]),
    (r"\.keys span\{[^}]*?gap:(\d+)px", [(1, "space", "key-hint-gap", None)]),
    (r"\.kbd\{[^}]*?padding:(\d+)px (\d+)px;[^}]*?border-bottom-width:(\d+)px", [(1, "space", "kbd-padding", 0), (2, "space", "kbd-padding", 1), (3, "layout", "kbd-bottom", None)]),
    (r"\.btn\{[^}]*?gap:(\d+)px", [(1, "space", "key-button-gap", None)]),
    (r"\.mgroup \.gh\{[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "match-head-padding", 0), (2, "space", "match-head-padding", 1)]),
    (r"\.mgroup dl\{[^}]*?grid-template-columns:(\d+)px minmax\(0,1fr\);gap:(\d+)px (\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, "layout", "match-key-width", None), (2, "space", "match-row-gap", 0), (3, "space", "match-row-gap", 1), (4, "space", "match-row-padding", 0), (5, "space", "match-row-padding", 1)]),
]
literals += check_literals(queue_literals)

# 库屏的库体检（.health / .htile / .lst、renderHealth 空态那一行、重复拷贝那张表）与差量账（票 24 的 .diff，挂单 Q1083）。
library_literals = [
    (r"\.health\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, "space", "health-grid-gap", None), (2, "space", "health-grid-padding", 0), (3, "space", "health-grid-padding", 1)]),
    (r"\.htile\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, "space", "health-tile-gap", None), (2, "space", "health-tile-padding", 0), (3, "space", "health-tile-padding", 1)]),
    (r"\.htile b\{[^}]*?font-size:(\d+)px", [(1, "font", "size-health-value", None)]),
    (r"\.lst>div\{[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "health-list-padding", 0), (2, "space", "health-list-padding", 1)]),
    (r'<div class="empty" style="padding:(\d+)px">扫描完成后生成体检报告', [(1, "space", "health-empty-padding", None)]),
    (r'<th style="width:(\d+)px">平台</th><th class="r" style="width:(\d+)px">份数</th><th class="r" style="width:(\d+)px">单份大小</th><th style="width:(\d+)px">', [(i + 1, "layout", "health-dup-columns", i) for i in range(4)]),
    (r"\.diff\{[^}]*?gap:(\d+)px", [(1, "space", "diff-gap", None)]),
    (r"\.diff div\{padding:(\d+)px (\d+)px", [(1, "space", "diff-tile-padding", 0), (2, "space", "diff-tile-padding", 1)]),
    (r"\.diff b\{[^}]*?font-size:(\d+)px", [(1, "font", "size-diff-value", None)]),
]
literals += check_literals(library_literals)

# 子库屏的弹层（DLG.subform 目标设置、DLG.excl 手动例外）与合并向导（.vrow、.ctbl）：表头列宽、输入框宽。
sublibrary_literals = [
    (r'<th style="width:(\d+)px">平台</th><th>设备直接能用</th><th style="width:(\d+)px">不能用时</th><th style="width:(\d+)px">覆盖</th>', [(i + 1, "layout", "platform-table-columns", i) for i in range(3)]),
    (r'<input class="input num" style="width:(\d+)px" data-dgi="capv"', [(1, "layout", "capacity-input-width", None)]),
    (r'<th>作品</th><th style="width:(\d+)px">平台</th><th class="r" style="width:(\d+)px">体积</th><th>备注</th><th style="width:(\d+)px">时间</th><th style="width:(\d+)px"></th>', [(i + 1, "layout", "exception-table-columns", i) for i in range(4)]),
    (r'<input class="input" style="width:(\d+)px" data-dgi="note"', [(1, "layout", "exception-note-width", None)]),
    (r"\.vrow\{[^}]*?grid-template-columns:\d+px minmax\(0,1fr\) (\d+)px (\d+)px (\d+)px (\d+)px", [(i + 1, "layout", "merge-row-columns", i) for i in range(4)]),
    (r'<table class="tbl ctbl"><thead><tr><th style="width:(\d+)px">字段</th>', [(1, "layout", "conflict-key-width", None)]),
]
literals += check_literals(sublibrary_literals)

# 作品详情页（票 gui-looks-like-the-design/15）：顶条、六个面、头上那一块与字卡、概览、变体卡、文件表、识别依据、
# 元数据那一面、标题面、媒体那一面。
work_literals = [
    (r"\.wdbar\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, "space", "work-bar-gap", None), (2, "space", "work-bar-padding", 0), (3, "space", "work-bar-padding", 1)]),
    (r"\.tabs\{[^}]*?gap:(\d+)px;padding:0 (\d+)px", [(1, "space", "tabs-gap", None), (2, "space", "tabs-padding", None)]),
    (r"\.tabs button\{height:(\d+)px;padding:0 (\d+)px", [(1, "layout", "tab-height", None), (2, "space", "tab-padding", None)]),
    (r'\.tabs button\[aria-selected="true"\]\{[^}]*?inset 0 -(\d+)px 0 var\(--accent\)', [(1, "layout", "tab-underline", None)]),
    (r"\.tabs button small\{[^}]*?margin-left:(\d+)px", [(1, "space", "tab-count-gap", None)]),
    (r"\.tabp\{padding:(\d+)px (\d+)px (\d+)px", [(i + 1, "space", "tab-panel-padding", i) for i in range(3)]),
    (r"\.hero\{[^}]*?grid-template-columns:(\d+)px minmax\(0,1fr\);gap:(\d+)px;padding:(\d+)px (\d+)px (\d+)px;[^}]*?var\(--pc\) (\d+%)", [(1, "layout", "hero-cover-width", None), (2, "space", "hero-gap", None), (3, "space", "hero-padding", 0), (4, "space", "hero-padding", 1), (5, "space", "hero-padding", 2), (6, "mix", "hero-tint", None)]),
    (r"\.htitle\{font-size:(\d+)px", [(1, "font", "size-hero", None)]),
    (r"\.mast h1\{font-size:(\d+)px", [(1, "font", "size-hero", None)]),
    (r"\.hfacts\{[^}]*?repeat\((\d+),minmax\(0,1fr\)\);gap:(\d+)px (\d+)px;[^}]*?max-width:(\d+)px", [(1, "layout", "hero-facts-columns", None), (2, "space", "hero-facts-gap", 0), (3, "space", "hero-facts-gap", 1), (4, "layout", "hero-facts-max", None)]),
    (r"\.hfacts div\{[^}]*?gap:(\d+)px", [(1, "space", "hero-fact-gap", None)]),
    (r"\.hcover \.tcard\{padding:(\d+)px (\d+)px", [(1, "space", "hero-card-padding", 0), (2, "space", "hero-card-padding", 1)]),
    (r"(?m)^\.tcard\{[^}]*?gap:(\d+)px", [(1, "space", "hero-card-gap", None)]),
    (r"(?m)^\.tc-t\{font-size:(\d+)px;[^}]*?-webkit-line-clamp:(\d+)", [(1, "font", "size-hero-card-title", None), (2, "layout", "hero-card-title-rows", None)]),
    (r"\.tc-s\{[^}]*?-webkit-line-clamp:(\d+)", [(1, "layout", "card-subtitle-rows", None)]),
    (r"\.dcover \.tc-t\{[^}]*?-webkit-line-clamp:(\d+)", [(1, "layout", "card-title-rows", None)]),
    (r"\.tc-wm\{[^}]*?right:-(\d+)px;bottom:-(\d+)px;font-size:(\d+)px", [(1, "layout", "hero-card-mark-offset", 0), (2, "layout", "hero-card-mark-offset", 1), (3, "font", "size-hero-card-mark", None)]),
    (r"\.ov\{[^}]*?grid-template-columns:minmax\(0,([\d.]+)fr\) minmax\(0,([\d.]+)fr\);gap:(\d+)px", [(1, "layout", "overview-columns", 0), (2, "layout", "overview-columns", 1), (3, "space", "overview-gap", None)]),
    (r"\.mstrip\{[^}]*?repeat\((\d+),1fr\);gap:(\d+)px", [(1, "layout", "media-strip-columns", None), (2, "space", "media-strip-gap", None)]),
    (r"\.desc\{font-size:([\d.]+)px;line-height:([\d.]+)", [(1, "font", "size-desc", None), (2, "font", "desc-line-height", None)]),
    (r"\.sect\{[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "section-card-padding", 0), (2, "space", "section-card-padding", 1)]),
    (r"\.sect>\.row:first-child\{margin-bottom:(\d+)px", [(1, "space", "section-card-title-gap", None)]),
    (r"\.sect h3\{font-size:([\d.]+)px", [(1, "font", "size-section-title", None)]),
    (r"\.srcb\{[^}]*?height:(\d+)px;padding:0 (\d+)px", [(1, "layout", "source-badge-height", None), (2, "space", "source-badge-padding", None)]),
    (r"\.infol\{[^}]*?gap:(\d+)px (\d+)px", [(1, "space", "info-list-gap", 0), (2, "space", "info-list-gap", 1)]),
    (r"\.vcard\{[^}]*?margin-bottom:(\d+)px", [(1, "space", "work-card-gap", None)]),
    (r"\.vhead\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, "space", "work-card-head-gap", None), (2, "space", "work-card-head-padding", 0), (3, "space", "work-card-head-padding", 1)]),
    (r"\.vbody\{[^}]*?minmax\(0,([\d.]+)fr\) minmax\(0,([\d.]+)fr\);gap:\d+px (\d+)px;padding:(\d+)px (\d+)px", [(1, "layout", "work-card-columns", 0), (2, "layout", "work-card-columns", 1), (3, "space", "work-card-columns-gap", None), (4, "space", "work-card-body-padding", 0), (5, "space", "work-card-body-padding", 1)]),
    (r"\.ftbl th,\.ftbl td\{[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "file-table-cell-padding", 0), (2, "space", "file-table-cell-padding", 1)]),
    (r'(?s)function evTab\(\).*?<div style="padding:(\d+)px (\d+)px" class="col">', [(1, "space", "evidence-card-padding", 0), (2, "space", "evidence-card-padding", 1)]),
    (r"\.mrow\{[^}]*?grid-template-columns:(\d+)px minmax\(0,1fr\) (\d+)px;gap:(\d+)px (\d+)px;padding:(\d+)px (\d+)px", [(1, "layout", "meta-row-columns", 0), (2, "layout", "meta-row-columns", 1), (3, "space", "meta-row-gap", 0), (4, "space", "meta-row-gap", 1), (5, "space", "meta-row-padding", 0), (6, "space", "meta-row-padding", 1)]),
    # 名那一列底下那行小字：字重是稿上唯一写明的常规字重（400），与底下那道间距一起核。
    (r"\.mrow \.fl small\{[^}]*?font-weight:(\d+);[^}]*?margin-top:(\d+)px", [(1, "font", "weight-regular", None), (2, "space", "meta-label-gap", None)]),
    (r"\.mrow \.v\{[^}]*?line-height:([\d.]+)", [(1, "font", "meta-value-line-height", None)]),
    (r"\.mrow \.v\.clamp\{[^}]*?-webkit-line-clamp:(\d+)", [(1, "layout", "meta-clamp-rows", None)]),
    (r"\.mrow\.dirty\{[^}]*?var\(--accent\) (\d+%)", [(1, "mix", "dirty-row-tint", None)]),
    (r"\.alts\{[^}]*?gap:(\d+)px", [(1, "space", "alts-gap", None)]),
    (r"\.alt\{[^}]*?gap:(\d+)px;[^}]*?padding:(\d+)px (\d+)px", [(1, "space", "alt-gap", None), (2, "space", "alt-padding", 0), (3, "space", "alt-padding", 1)]),
    (r"\.pickchips\{[^}]*?gap:(\d+)px", [(1, "space", "pick-chip-gap", None)]),
    # .pickchip 的 padding 是「上 右 下 左」＝ 0 8px 0 3px；令牌 pick-chip-padding 记的是（左, 右）。
    (r"\.pickchip\{[^}]*?gap:(\d+)px;[^}]*?height:(\d+)px;padding:0 (\d+)px 0 (\d+)px", [(1, "space", "pick-chip-gap", None), (2, "layout", "pick-chip-height", None), (3, "space", "pick-chip-padding", 1), (4, "space", "pick-chip-padding", 0)]),
    (r"\.pickchip b\{[^}]*?max-width:(\d+)px", [(1, "layout", "pick-chip-max", None)]),
    (r"\.savebar\{[^}]*?gap:(\d+)px;padding:(\d+)px (\d+)px", [(1, "space", "save-bar-gap", None), (2, "space", "save-bar-padding", 0), (3, "space", "save-bar-padding", 1)]),
    (r'<textarea class="input" data-mf="\$\{k\}" name="mf-\$\{k\}" rows="(\d+)"', [(1, "layout", "meta-textarea-rows", None)]),
    (r"\.tadd\{[^}]*?minmax\(0,1fr\) (\d+)px (\d+)px auto", [(1, "layout", "title-add-columns", 0), (2, "layout", "title-add-columns", 1)]),
    (r'<div class="row" style="padding:(\d+)px 0;border-top:1px solid var\(--line\)"><span>\$\{esc\(r\.v\)\}', [(1, "space", "suppressed-row-padding", None)]),
    (r"\.mgrid\{[^}]*?minmax\((\d+)px,1fr\)\);gap:(\d+)px", [(1, "layout", "media-tile-min", None), (2, "space", "media-grid-gap", None)]),
    (r"\.mtile \.mi\{padding:(\d+)px (\d+)px", [(1, "space", "media-info-padding", 0), (2, "space", "media-info-padding", 1)]),
    (r"\.mtile \.pv \.play\{[^}]*?font-size:(\d+)px", [(1, "font", "size-play-mark", None)]),
    (r"\.noff\{[^}]*?gap:(\d+)px;padding:(\d+)px;[^}]*?var\(--sunken\) 0 (\d+)px", [(1, "space", "noff-gap", None), (2, "space", "noff-padding", None), (3, "layout", "noff-stripe", None)]),
    (r"\.noff i\{[^}]*?font-size:(\d+)px", [(1, "font", "size-noff-mark", None)]),
]
literals += check_literals(work_literals)

# 弹层与弹出菜单的阴影（[shadow.pop]，票 gui-looks-like-the-design/04 裁定）：设计稿 --pop 第一层是
# 「0 10px 30px -12px」。偏移照稿；扩散照「egui 只收非负数」换算成 max(0, 稿上的扩散)；模糊是看图调出来的近似，
# 进豁免表——但它是对着稿上 30 模糊、-12 扩散这一组调的，稿上这组数一变，近似就得重调，所以把这组数钉在这儿。
POP_TUNED_AGAINST = (30, -12)
m = re.search(r"--pop:(-?\d+)(?:px)? (-?\d+)px (-?\d+)px (-?\d+)px var\(--pop-color\)", html)
if not m:
    problems.append("找不到 --pop 的第一层")
else:
    pop = tokens["shadow"]["pop"]
    for got, want, key in [(int(m[1]), pop["offset"][0], "shadow.pop.offset 横"), (int(m[2]), pop["offset"][1], "shadow.pop.offset 竖"), (max(0, int(m[4])), pop["spread"], "shadow.pop.spread（取 max(0, 稿上的扩散)）")]:
        literals += 1
        if got != want:
            problems.append(f"{key}: 设计稿折出来是 {got}，令牌是 {want}")
    literals += 1
    if (int(m[3]), int(m[4])) != POP_TUNED_AGAINST:
        problems.append(f"shadow.pop.blur: 设计稿 --pop 的模糊与扩散成了 {m[3]}px {m[4]}px，令牌里那个 {raw_tokens['shadow']['pop']['blur']} 是对着 {POP_TUNED_AGAINST[0]}px {POP_TUNED_AGAINST[1]}px 调的，重调")

# 颜色里不走 :root 那几格：表里没有的平台（设计稿 --pc:${PCOL[…]||'#555'} 那个兜底）、视频格上的播放标
# （设计稿 .mtile .pv .play 的 background 与 color）。
fallbacks = set(re.findall(r"--pc:\$\{PCOL\[[^\]]+\]\|\|'(#[0-9A-Fa-f]+)'\}", html))
literals += 1
if len(fallbacks) != 1 or not same_color(next(iter(fallbacks)), tokens["color"]["platform"]["other"]):
    problems.append(f"平台色 other: 设计稿 --pc 的兜底是 {sorted(fallbacks)}，令牌是 {tokens['color']['platform']['other']}")
m = re.search(r"\.mtile \.pv \.play\{[^}]*?background:(rgba\([^)]*\));color:(#[0-9A-Fa-f]+)", html)
if not m:
    problems.append("找不到 .mtile .pv .play 的 background 与 color")
else:
    for got, key in [(m[1], "shade"), (m[2], "mark")]:
        literals += 1
        want = tokens["color"]["video"][key]
        if not same_color(got, want):
            problems.append(f"视频播放标 {key}: 设计稿 .mtile .pv .play 是 {got}，令牌是 {want}")

# 平台标上的字（设计稿 .hplat 与卡面上的 .cv-plat 的 color）、开关里的圆点（.switch i::after 的 background）：
# 稿上都写死在规则上、暗色主题不另写，令牌里两套主题共用一格（票 gate-and-tests/06）。
for selector, prop, section, key, what in [
    (".hplat", "color", "platform-badge", "ink", "平台标上的字"),
    (".cv-plat", "color", "platform-badge", "ink", "卡面平台标上的字"),
    (".switch i::after", "background", "switch", "knob", "开关里的圆点"),
]:
    m = re.search(re.escape(selector) + r"\{(?:[^}]*?;)?" + prop + r":(#[0-9A-Fa-f]+)", html)
    literals += 1
    want = tokens["color"][section][key]
    if not m:
        problems.append(f"{what}: 找不到 {selector} 的 {prop}")
    elif not same_color(m[1], want):
        problems.append(f"{what} {section}.{key}: 设计稿 {selector} 的 {prop} 是 {m[1]}，令牌是 {want}")

# 字体族：等宽照设计稿 --mono-latin 逐个对上（去掉末尾的通用族 monospace）；无衬线只挑了设计稿 --sans 里的几个，
# 核的是「每一个都在稿里、先后次序一样」。
root_css = css_block(r":root")


def families(value: str) -> list[str]:
    return [name.strip().strip('"') for name in value.split(",")]


mono_design = [name for name in families(root_css.get("mono-latin", "")) if name != "monospace"]
literals += 1
if list(tokens["font"]["mono"]) != mono_design:
    problems.append(f"font.mono: 设计稿 --mono-latin 是 {mono_design}，令牌是 {list(tokens['font']['mono'])}")
sans_design = iter(families(root_css.get("sans", "")))
literals += 1
if not all(name in sans_design for name in tokens["font"]["sans"]):
    problems.append(f"font.sans: 令牌 {list(tokens['font']['sans'])} 不全在设计稿 --sans 里，或者先后次序不一样")

# ── 豁免表：设计稿里没有可核的对应物的令牌 ──
# 键是令牌的格名（节.键；整个数组写键名），值是一句理由：说清**设计稿里为什么没有可核的对应物**，
# 不是「还没顾上核」。设计稿里有对应物的，写一条核对，不进这张表。
EXEMPT: dict[str, str] = {
    "version": "令牌文件格式的版本号（程序读令牌时核它），不是一个样式参数，设计稿里没有这一说。",
    "font.bold-coverage": "字体预算的裁定（粗体只覆盖拉丁与数字），是个枚举；设计稿是拿 @font-face「CJK 常规」的 unicode-range 把中文钉在常规体来画出这个结果，没有一个能与之比的值。",
    "layout.rail-collapse-below": "设计稿的窗口定宽（.win 的 min-width:1160px），左栏只靠「收起」按钮切换，没有「窄到多少自动收」这一格；800 是拿主意的人 2026-09-14 定、照 layout.rs 那三个数算出来的。",
    "layout.card-info-height": "设计稿卡片封面下那三行是自然流排出来的（.gcard 的 gap、.gt 两行的 min-height:2.8em、.gm 一行），没有写成一个高；104 是界面为了等高行另定的。",
    "layout.card-group-height": "设计稿组头 .ghead 的高是 padding:10px 0 8px 加字撑出来的，没有写成一个数。",
    "layout.fold-mark": "设计稿的折叠标是 .iconbtn 里 13px 字号的一个 ▾ 字符，稿里只有字号没有宽；7 是照稿图量出来的。",
    "layout.root-name-max": "设计稿根那张表没有给根名称那一列设上限；120 是拿主意的人 2026-09-14 定的，稿上没有这一格。",
    "layout.health-list-max-height": "设计稿体检明细列表 .lst 没写最大高度（稿里数据少，不用滚）；420 是拿主意的人 2026-09-15 答照稿用虚拟化列表时另定的。",
    "layout.settings-name-width": "设计稿设置屏上没有「主库原名」那一格（改名是票 gui-looks-like-the-design/31 补的落点），无从核起。",
    "layout.radio-diameter": "设计稿单选框是浏览器原生的 <input type=radio>（.opt input 只设了 accent-color），13px 是浏览器缺省，CSS 里没写。",
    "layout.radio-dot": "同上：选中那一粒是浏览器按 accent-color 画出来的，CSS 里没有这一格。",
    "layout.radio-gap": "同上：圆心与外圈之间那道缝是浏览器原生单选框画出来的样子，CSS 里没有这一格。",
    "layout.title-name-share": "设计稿标题面那张表（titleTab 里的 .tbl）没写列宽；0.30 是协调人 2026-09-15 照稿图定的比例。",
    "space.steps": "通用间距的档位，界面随手要一个间距时从这几档里取（由代码走查守，见 tokens.rs 的 Space）；设计稿没有一处列出这几档。脚本只拿它验 .stage / .nextline 的 gap 落在档里——那是核稿，不是核这几档，不算核到。",
    "shadow.pop.blur": "稿上 --pop 的模糊是 30、扩散是 -12，两个一起才画出「只落在下沿」的样子；egui 的扩散收不了负数，只好把模糊收窄来凑，24 是对着稿看出来的，不是从 30 算出来的，没有一个等值可核（票 gui-looks-like-the-design/04 裁定）。换算前提由上面 POP_TUNED_AGAINST 那条钉着。",
}


def leaves(path: str, value):
    """令牌文件里的每一格：标量一格，数组逐格。"""
    if isinstance(value, dict):
        for key, inner in value.items():
            yield from leaves(f"{path}{key}." if isinstance(inner, dict) else f"{path}{key}", inner)
    elif isinstance(value, list):
        for i in range(len(value)):
            yield f"{path}[{i}]"
    else:
        yield path


all_leaves = list(leaves("", raw_tokens))


def exempt_by(leaf: str):
    """这一格被豁免表里哪一条管着：它自己，或者它所在的那个数组。"""
    for name in (leaf, leaf.split("[")[0]):
        if name in EXEMPT:
            return name
    return None


def squeeze(names: list[str]) -> list[str]:
    """一个数组的每一格都在名单上，就只写键名。"""
    rows: dict[str, list[str]] = {}
    for name in names:
        rows.setdefault(name.split("[")[0], []).append(name)
    out = []
    for key, items in rows.items():
        whole = [leaf for leaf in all_leaves if leaf.startswith(key + "[")]
        out.extend([key] if whole and len(items) == len(whole) else items)
    return out


missing = [leaf for leaf in all_leaves if leaf not in covered and exempt_by(leaf) is None]
both = [leaf for leaf in all_leaves if leaf in covered and exempt_by(leaf) is not None]
used = {exempt_by(leaf) for leaf in all_leaves}
stale = [name for name in EXEMPT if name not in used]
blank = [name for name, why in EXEMPT.items() if not why.strip()]
if missing:
    problems.append(
        f"没人核的令牌 {len(missing)} 格（写一条核对，或者进 EXEMPT 并写明设计稿里为什么没有可核的对应物）：\n  "
        + "\n  ".join(squeeze(missing))
    )
if both:
    problems.append("既核了又在豁免表里（删掉豁免那一条）：" + "、".join(squeeze(both)))
if stale:
    problems.append("豁免表里有、令牌文件里没有（令牌删了或改了名，豁免那一条跟着删）：" + "、".join(stale))
if blank:
    problems.append("豁免表里没写理由：" + "、".join(blank))

if problems:
    print("\n".join(problems))
    sys.exit(1)
n = sum(len(tokens["color"][t]) for t in ("light", "dark")) + len(pcol) + 3 + buttons + literals
hit = sum(leaf in covered for leaf in all_leaves)
print(f"一致：核对了 {n} 项；令牌 {len(all_leaves)} 格，核到 {hit} 格、豁免 {len(all_leaves) - hit} 格")
