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

# 任务屏（票 gui-looks-like-the-design/25）：屏头、屏体、块与块、卡片、表格、空态的留白。
space, layout = tokens["space"], tokens["layout"]
for pattern, wants in [
    (r"\.scrhead\{[^}]*?padding:(\d+)px (\d+)px", [("screen-head-padding 上下", space["screen-head-padding"][0]), ("screen-head-padding 左右", space["screen-head-padding"][1])]),
    (r"\.scrbody\{[^}]*?padding:(\d+)px (\d+)px (\d+)px", [("screen-body-padding 上", space["screen-body-padding"][0]), ("screen-body-padding 左右", space["screen-body-padding"][1]), ("screen-body-padding 下", space["screen-body-padding"][2])]),
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

# 半号字号两档（拿主意的人裁，挂单 Q840）：设计稿里写 11.5 与 12.5 的字面值——表头 .tbl th、屏头说明 .scrhead .sub、表格行 .hist td。
for pattern, key in [
    (r"\.tbl th\{[^}]*?font-size:([\d.]+)px", "size-caption-plus"),
    (r"\.scrhead \.sub\{[^}]*?font-size:([\d.]+)px", "size-small-plus"),
    (r"\.hist td\{[^}]*?font-size:([\d.]+)px", "size-small-plus"),
]:
    m = re.search(pattern, html)
    if not m:
        problems.append(f"找不到 {pattern}")
        continue
    literals += 1
    if float(m[1]) != tokens["font"][key]:
        problems.append(f"{key}: 设计稿是 {m[1]}px，令牌是 {tokens['font'][key]}")

if problems:
    print("\n".join(problems))
    sys.exit(1)
n = sum(len(tokens["color"][t]) for t in ("light", "dark")) + len(pcol) + 3 + buttons + literals
print(f"一致：核对了 {n} 项")
