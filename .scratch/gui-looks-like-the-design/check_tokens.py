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

# 两档半号字号（拿主意的人定：照稿加，各分支同一个键名）：设计稿 .tbl th 的 11.5px、.note 的 12.5px。
for pattern, key in [
    (r"\.tbl th\{[^}]*?font-size:([\d.]+)px", "size-caption-plus"),
    (r"\.note\{[^}]*?font-size:([\d.]+)px", "size-small-plus"),
]:
    m = re.search(pattern, html)
    if not m:
        problems.append(f"找不到 {key} 在设计稿里的那个字面值")
        continue
    literals += 1
    if float(m[1]) != float(tokens["font"][key]):
        problems.append(f"{key}: 设计稿是 {m[1]}px，令牌是 {tokens['font'][key]}")

# 子库屏空态那张卡的标题字号：设计稿 renderDevs 里写在 h3 上的字面值。
m = re.search(r'<h3 style="font-size:(\d+)px">还没有子库</h3>', html)
if not m:
    problems.append("找不到子库屏空态卡标题的 font-size")
else:
    literals += 1
    if int(m[1]) != tokens["font"]["size-empty-title"]:
        problems.append(f"size-empty-title: 设计稿是 {m[1]}px，令牌是 {tokens['font']['size-empty-title']}")

if problems:
    print("\n".join(problems))
    sys.exit(1)
n = sum(len(tokens["color"][t]) for t in ("light", "dark")) + len(pcol) + 3 + buttons + literals
print(f"一致：核对了 {n} 项")
