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
    global literals
    literals += 1
    if float(got) not in [float(s) for s in tokens["space"]["steps"]]:
        problems.append(f"{what} {got}px 不在间距档位 {tokens['space']['steps']} 里")


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
if m := literal(r"\.tbl th\{[^}]*?font-size:([\d.]+)px", ".tbl th 的 font-size"):
    same(m[1], tokens["font"]["size-caption-plus"], "size-caption-plus（.tbl th）")
if m := literal(r"\.stage\.next\{[^}]*?inset (\d+)px", ".stage.next 的竖条"):
    same(m[1], tokens["layout"]["row-stripe"], "row-stripe（.stage.next）")
if m := literal(r"\.tbl td\.st\{box-shadow:inset (\d+)px", ".tbl td.st 的竖条"):
    same(m[1], tokens["layout"]["row-stripe"], "row-stripe（.tbl td.st）")
if m := literal(r"\.nextline\{[^}]*?gap:(\d+)px;[^}]*?padding:(\d+)px;", ".nextline 的 gap 与 padding"):
    in_steps(m[1], ".nextline 的 gap")
    same(m[2], panel_padding[1], "下一步那一块的内边距（panel-padding 左右）")
if m := literal(r"\.iconbtn\{width:(\d+)px", ".iconbtn 的 width"):
    same(m[1], tokens["layout"]["icon-button"], "icon-button")
if m := literal(r'style="width:(\d+)px" aria-label="前端格式"', "导出设置那一块前端格式下拉的 width"):
    same(m[1], tokens["layout"]["format-select-width"], "format-select-width")

if problems:
    print("\n".join(problems))
    sys.exit(1)
n = sum(len(tokens["color"][t]) for t in ("light", "dark")) + len(pcol) + 3 + buttons + literals
print(f"一致：核对了 {n} 项")
