"""核对设计稿 prototype.html 顶部的 CSS 变量与令牌是否一致。

令牌全仓库只有一份：crates/gui/src/tokens.toml——界面编进二进制的就是它。设计稿目录里
不留第二份：两份各改各的，正是这份脚本要防的事。

用法：python3 check_tokens.py    （在哪个目录下运行都行；不一致时列出差异并以 1 退出）
"""
import re
import sys
import tomllib
from pathlib import Path

here = Path(__file__).resolve().parent
repo = here.parents[1]
tokens = tomllib.loads((repo / "crates" / "gui" / "src" / "tokens.toml").read_text(encoding="utf-8"))
html = (here / "prototype.html").read_text(encoding="utf-8")


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
    return v.upper()


problems = []
for theme, pattern in [("light", r":root"), ("dark", r':root\[data-theme="dark"\]')]:
    css = css_block(pattern)
    for key, want in tokens["color"][theme].items():
        got = css.get(key)
        if got is None:
            problems.append(f"{theme}: CSS 里没有 --{key}")
        elif norm(got) != norm(want):
            # 半透明遮罩允许 1 级取整误差
            if not (len(want) == 9 and norm(got)[:7] == want.upper()[:7] and abs(int(norm(got)[7:], 16) - int(want[7:], 16)) <= 1):
                problems.append(f"{theme}: --{key} 设计稿是 {got}，令牌是 {want}")

pcol = dict(re.findall(r"(\w+):'(#[0-9A-Fa-f]{6})'", re.search(r"const PCOL=\{(.*?)\}", html).group(1)))
for plat, want in tokens["color"]["platform"].items():
    if plat == "other":
        continue
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

# 任务屏（票 gui-looks-like-the-design/25）：块与块、卡片、表格、空态的留白（屏头、屏体的内边距归下面外壳那一段核）。
space, layout = tokens["space"], tokens["layout"]
for pattern, wants in [
    (r'id="s-task".*?class="scrbody col" style="gap:(\d+)px"', [("screen-section-gap", space["screen-section-gap"])]),
    (r'id="s-task".*?<div class="col" style="gap:(\d+)px"><span class="sec">正在运行', [("section-title-gap", space["section-title-gap"])]),
    (r"\.runcard\{padding:(\d+)px;[^}]*?gap:(\d+)px \d+px", [("card-padding", space["card-padding"]), ("card-row-gap", space["card-row-gap"])]),
    (r"\.runcard \.bar\{[^}]*?height:(\d+)px", [("runcard-progress", layout["runcard-progress"])]),
    (r"\.runcard \.meta\{[^}]*?gap:(\d+)px", [("meta-gap", space["meta-gap"])]),
    (r"\$\('#tqueue'\).*?class=\"card row\" style=\"padding:(\d+)px (\d+)px\"", [("queue-row-padding 上下", space["queue-row-padding"][0]), ("queue-row-padding 左右", space["queue-row-padding"][1])]),
    (r"\.empty\{padding:(\d+)px", [("empty-padding", space["empty-padding"])]),
    (r"\.tbl th\{[^}]*?padding:(\d+)px (\d+)px", [("table-head-padding 上下", space["table-head-padding"][0]), ("table-head-padding 左右", space["table-head-padding"][1])]),
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

# 两档半号字号（拿主意的人裁，挂单 Q840 / Q862）：设计稿 .scrhead .sub、.hist td 的 12.5px，.railfoot .st、.tbl th 的 11.5px。
for selector, key in [(r"\.scrhead \.sub\{", "size-small-plus"), (r"\.hist td\{", "size-small-plus"), (r"\.railfoot \.st\{", "size-caption-plus"), (r"\.tbl th\{", "size-caption-plus")]:
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
    """逐条核对设计稿里写死的字面值；交回核了几项。"""
    checked = 0
    for pattern, checks in entries:
        m = re.search(pattern, html)
        if not m:
            problems.append(f"找不到 {pattern}")
            continue
        for group, section, key, at in checks:
            checked += 1
            want = token(section, key, at)
            if float(m[group]) != float(want):
                where = key if at is None else f"{key}[{at}]"
                problems.append(f"{where}: 设计稿是 {m[group]}，令牌是 {want}")
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

if problems:
    print("\n".join(problems))
    sys.exit(1)
n = sum(len(tokens["color"][t]) for t in ("light", "dark")) + len(pcol) + 3 + buttons + literals
print(f"一致：核对了 {n} 项")
