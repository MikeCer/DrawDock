#!/usr/bin/env python3
"""Render pixel-exact radial-menu and canvas reference mockups from the icon + token sources.

Output: assets/mockups/*.png at 3x (1 dp = 3 px, ~xxhdpi). Requires cairosvg + Pillow.
Usage: python3 scripts/render_mockups.py   (run from the skill root)
These PNGs are visual references for implementation; draw the real UI in code with tokens.
"""
import math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from build_icons import ICONS
from build_tokens import COLORS, DIMENS
import cairosvg

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "assets/mockups"); os.makedirs(OUT, exist_ok=True)
S = 3  # px per dp
ICON = {n: p for _, n, _, p in ICONS}
FONT = "Lato, DejaVu Sans, sans-serif"

def c(name):
    v = COLORS[name]; a = int(v[:2], 16) / 255
    return f"#{v[2:]}", round(a, 3)
def fill(name): h, a = c(name); return f'fill="{h}" fill-opacity="{a}"'
def stroke(name): h, a = c(name); return f'stroke="{h}" stroke-opacity="{a}"'

def icon(name, cx, cy, size=24, color="#E6F0FF", opacity=1.0):
    k = size / 24; parts = []
    for d, kind, sw in ICON[name]:
        if kind == "fill": parts.append(f'<path d="{d}" fill="{color}" stroke="none"/>')
        else: parts.append(f'<path d="{d}" stroke-width="{sw}"/>')
    return (f'<g transform="translate({cx - size/2} {cy - size/2}) scale({k})" fill="none" stroke="{color}" '
            f'stroke-linecap="round" stroke-linejoin="round" opacity="{opacity}">' + "".join(parts) + "</g>")

def text(x, y, s, size=11, color="#E6F0FF", weight=500, anchor="middle", opacity=1.0):
    return (f'<text x="{x}" y="{y}" font-family="{FONT}" font-size="{size}" font-weight="{weight}" fill="{color}" '
            f'text-anchor="{anchor}" opacity="{opacity}">{s}</text>')

D = DIMENS
R_MENU = D["menu_diameter"] / 2; R_ITEM = D["menu_item_ring_radius"]; R_CENTER = D["menu_center_diameter"] / 2

def menu(cx, cy, items, center="close", center_label=None):
    """items: list of dict(icon,label,state in {normal,selected,pressed,disabled,on,off,danger}, swatch, width)"""
    g = []
    g.append(f'<circle cx="{cx}" cy="{cy+4}" r="{R_MENU}" fill="#000" fill-opacity="0.32" filter="url(#blur)"/>')
    g.append(f'<circle cx="{cx}" cy="{cy}" r="{R_MENU}" {fill("menu_surface")} {stroke("menu_ring_stroke")} stroke-width="1"/>')
    g.append(f'<circle cx="{cx}" cy="{cy}" r="{R_CENTER + 14}" fill="none" {stroke("menu_divider")} stroke-width="1"/>')
    n = len(items)
    for i, it in enumerate(items):
        a = -math.pi / 2 + 2 * math.pi * i / n
        x, y = cx + R_ITEM * math.cos(a), cy + R_ITEM * math.sin(a)
        st = it.get("state", "normal"); col = "#E6F0FF"; op = 1.0; lab = "#E6F0FF"
        hl = D["menu_item_highlight"] / 2
        if st == "selected":
            g.append(f'<circle cx="{x}" cy="{y-8}" r="17" {fill("menu_item_selected_bg")}/>'); col = c("menu_item_selected_icon")[0]; lab = c("brand_cyan")[0]
        elif st == "pressed":
            g.append(f'<circle cx="{x}" cy="{y}" r="{hl}" {fill("menu_item_pressed")}/>')
        elif st == "disabled":
            op = 0.38
        elif st == "on":
            col = lab = c("brand_cyan")[0]
        elif st == "danger":
            col = lab = c("danger")[0]
        if "swatch" in it:
            sw = it["swatch"]; r = D["swatch_diameter"] / 2
            if st == "selected":
                g = g[:-1]  # swatches use ring, not filled disc
                g.append(f'<circle cx="{x}" cy="{y-8}" r="{r+4}" fill="none" {stroke("brand_cyan")} stroke-width="3"/>'); lab = c("brand_cyan")[0]
            g.append(f'<circle cx="{x}" cy="{y-8}" r="{r}" fill="#{COLORS[sw][2:]}" '
                     + (f'{stroke("swatch_outline")} stroke-width="1.5"' if sw in ("ink_black",) else 'stroke="none"') + '/>')
            if st == "selected":
                ck = "#0B1B3A" if sw in ("ink_white", "ink_yellow") else "#FFFFFF"
                g.append(f'<path d="M{x-6} {y-8}l4 4 8-8" fill="none" stroke="{ck}" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>')
        elif "width" in it:
            w = it["width"]; inkc = "#" + COLORS[it.get("ink", "ink_blue")][2:]
            if st == "selected":
                g = g[:-1]; g.append(f'<circle cx="{x}" cy="{y-8}" r="17" fill="none" {stroke("brand_cyan")} stroke-width="2"/>')
            g.append(f'<path d="M{x-11} {y-8}h22" stroke="{inkc}" stroke-width="{w}" stroke-linecap="round"/>')
        else:
            g.append(icon(it["icon"], x, y - 8, 24, col, op))
            if st == "on":
                g.append(f'<rect x="{x+9}" y="{y-24}" width="16" height="10" rx="5" {fill("brand_cyan")}/>'
                         + text(x + 17, y - 16.2, "ON", 6.5, "#0B1B3A", 800))
        if it.get("badge"):
            g.append(f'<circle cx="{x}" cy="{y+29}" r="2.5" {fill("brand_cyan")}/>')
        g.append(text(x, y + 22, it["label"], D["menu_label_sp"], lab, 600 if st == "selected" else 500, opacity=op))
    g.append(f'<circle cx="{cx}" cy="{cy}" r="{R_CENTER}" {fill("menu_center_surface")} {stroke("menu_center_stroke")} stroke-width="1.5"/>')
    if center_label:
        g.append(icon(center, cx, cy - 8, 22)); g.append(text(cx, cy + 16, center_label, D["menu_center_label_sp"], "#8FA3C4", 600))
    else:
        g.append(icon(center, cx, cy, 24))
    return "".join(g)

DEFS = '<defs><filter id="blur" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="8"/></filter>' \
       '<pattern id="grid" width="24" height="24" patternUnits="userSpaceOnUse"><path d="M24 0H0V24" fill="none" stroke="#0B1B3A" stroke-opacity="0.1"/></pattern>' \
       '<pattern id="gridd" width="24" height="24" patternUnits="userSpaceOnUse"><path d="M24 0H0V24" fill="none" stroke="#FFF" stroke-opacity="0.12"/></pattern></defs>'

def save(name, w, h, body, bg="#E9EEF5", title=None, note=None):
    t = ""
    if title: t += text(16, 26, title, 15, "#0B1B3A", 700, "start")
    if note: t += text(16, h - 14, note, 10, "#41557A", 500, "start")
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{DEFS}'
           f'<rect width="{w}" height="{h}" fill="{bg}"/>' + body + t + "</svg>")
    cairosvg.svg2png(bytestring=svg.encode(), write_to=os.path.join(OUT, name + ".png"), output_width=w * S, output_height=h * S)

def pen_tip(x, y):
    return (f'<g transform="translate({x} {y}) rotate(22)"><path d="M0 0l-5 -16h10z" fill="#1C3566"/>'
            f'<rect x="-6" y="-100" width="12" height="84" rx="5" fill="#13284F"/><circle r="2" fill="#22D3EE"/></g>')

ROOT_ITEMS = lambda **o: [
    dict(icon="pen", label="Pen", state=o.get("pen", "normal"), badge=o.get("pen_badge", True)),
    dict(icon="shapes", label="Shapes"), dict(icon="select_edit", label="Select / Edit"),
    dict(icon="style", label="Style"), dict(icon="board_grid", label="Board / Grid"),
    dict(icon="undo", label="Undo", state=o.get("undo", "normal")),
    dict(icon="redo", label="Redo", state=o.get("redo", "disabled")),
    dict(icon="more", label="More"),
]

def screens():
    W, H, cx, cy = 340, 360, 170, 186
    sub = {
     "01-root": (ROOT_ITEMS(), "close", None, "Root: 8 items clockwise from top. Dot = active tool family. Redo disabled."),
     "02-pen": ([dict(icon="freehand", label="Freehand", state="selected"), dict(icon="highlighter", label="Highlighter"),
                 dict(icon="laser_pointer", label="Laser pointer")], "back", "Pen", "Pen submenu: active tool = cyan disc. Back in center."),
     "03-shapes": ([dict(icon=n, label=l, state="selected" if n == "rectangle" else "normal") for n, l in
                    [("rectangle", "Rectangle"), ("circle", "Circle"), ("ellipse", "Ellipse"), ("line", "Line"),
                     ("polygon", "Polygon"), ("polyline", "Polyline"), ("arrow", "Arrow"), ("text", "Text")]], "back", "Shapes", "Shapes submenu (8 items)."),
     "04-select": ([dict(icon=n, label=l) for n, l in [("select_move", "Move"), ("rotate", "Rotate"), ("resize", "Resize"),
                    ("mirror_horizontal", "Mirror H"), ("mirror_vertical", "Mirror V"), ("deselect", "Deselect")]], "back", "Edit",
                   "Select / Edit: short labels; full names as content descriptions."),
     "05-style": ([dict(icon="color", label="Color", badge=True), dict(icon="width", label="Width")], "back", "Style", "Style submenu: both items open nested submenus."),
     "06-color": ([dict(label=l, swatch=k, state="selected" if k == "ink_blue" else "normal") for k, l in
                   [("ink_red", "Red"), ("ink_orange", "Orange"), ("ink_yellow", "Yellow"), ("ink_green", "Green"),
                    ("ink_blue", "Blue"), ("ink_black", "Black"), ("ink_white", "White")]], "back", "Color", "Style > Color: 7 fixed swatches; selected = ring + check."),
     "07-width": ([dict(label=l, width=w, state="selected" if w == 6 else "normal") for l, w in [("Thin", 3), ("Medium", 6), ("Thick", 12)]],
                  "back", "Width", "Style > Width: preview at true dp width, current color."),
     "08-board": ([dict(icon="blackboard", label="Blackboard", state="on"), dict(icon="grid", label="Grid")], "back", "Board",
                  "Board / Grid: independent toggles; ON = cyan icon + ON pill."),
     "09-more": ([dict(icon="mode_pointer", label="Pointer mode"), dict(icon="finish_path", label="Finish path", state="disabled"),
                  dict(icon="cancel_path", label="Cancel path", state="disabled"), dict(icon="paste_image", label="Paste image"),
                  dict(icon="clear_all", label="Clear all", state="danger"), dict(icon="settings", label="Settings")], "back", "More",
                 "More (idle): mode item shows target mode; path items disabled."),
     "10-more-path": ([dict(icon="mode_pointer", label="Pointer mode"), dict(icon="finish_path", label="Finish path"),
                  dict(icon="cancel_path", label="Cancel path"), dict(icon="paste_image", label="Paste image"),
                  dict(icon="clear_all", label="Clear all", state="danger"), dict(icon="settings", label="Settings")], "back", "More",
                 "More while building a polygon/polyline: path items enabled."),
    }
    for k, (items, ctr, lab, note) in sub.items():
        save("menu-" + k, W, H, menu(cx, cy, items, ctr, lab), title=None, note=note)

def anatomy():
    W, H, cx, cy = 680, 420, 230, 215
    b = menu(cx, cy, ROOT_ITEMS(pen="normal", undo="pressed"))
    a = "#FF6B57"
    def call(x1, y1, x2, y2, s):
        return (f'<path d="M{x1} {y1}L{x2} {y2}" stroke="{a}" stroke-width="1"/><circle cx="{x1}" cy="{y1}" r="2.5" fill="{a}"/>'
                + text(x2 + 6, y2 + 4, s, 11, "#0B1B3A", 600, "start"))
    b += f'<circle cx="{cx}" cy="{cy}" r="{R_ITEM}" fill="none" stroke="{a}" stroke-dasharray="4 4" stroke-width="1"/>'
    b += call(cx + R_MENU * 0.707, cy - R_MENU * 0.707, 420, 50, "Surface \u00d8 288 dp, navy 94%, 1 dp cyan ring")
    b += call(cx + 3, cy - R_ITEM + 29, 420, 100, "Active tool family: 5 dp cyan dot")
    b += call(cx + R_ITEM * 0.707 + 8, cy - R_ITEM * 0.707 - 8, 420, 150, "Icon 24 dp + label 11 sp, slot 64 dp")
    b += call(cx + R_ITEM, cy, 420, 215, "Item centers on r = 100 dp, 45\u00b0 apart")
    b += call(cx + R_CENTER * 0.7, cy + R_CENTER * 0.7, 420, 280, "Center Close / Back \u00d8 64 dp")
    b += call(cx - R_ITEM * 0.707, cy + R_ITEM * 0.707, 420, 345, "Pressed: \u00d8 56 dp, cyan 22%")
    save("menu-00-anatomy", W, H, b, title="Radial menu anatomy (root)")

def canvas():
    W, H = 640, 300
    b = '<rect x="16" y="40" width="296" height="230" rx="10" fill="#FFF"/><rect x="16" y="40" width="296" height="230" rx="10" fill="url(#grid)"/>'
    b += '<rect x="328" y="40" width="296" height="230" rx="10" fill="#1F2B27"/><rect x="328" y="40" width="296" height="230" rx="10" fill="url(#gridd)"/>'
    for ox, ink in [(16, "#3B82F6"), (328, "#FACC15")]:
        x0, y0, w, h = ox + 70, 120, 150, 100
        b += f'<path d="M{x0+20} {y0+70}c30-60 60-60 80-20s40 10 40-30" fill="none" stroke="{ink}" stroke-width="6" stroke-linecap="round"/>'
        b += f'<rect x="{x0}" y="{y0}" width="{w}" height="{h}" fill="none" stroke="#22D3EE" stroke-width="1.5" stroke-dasharray="6 4"/>'
        b += f'<path d="M{x0+w/2} {y0}V{y0-32}" stroke="#22D3EE" stroke-width="1.5"/>'
        b += f'<circle cx="{x0+w/2}" cy="{y0-32}" r="8" fill="#FFF" stroke="#22D3EE" stroke-width="2"/>' + icon("rotate", x0 + w/2, y0 - 32, 11, "#0B1B3A")
        for hx, hy in [(x0, y0), (x0+w, y0), (x0, y0+h), (x0+w, y0+h)]:
            b += f'<rect x="{hx-6}" y="{hy-8}" width="12" height="12" rx="2" fill="#FFF" stroke="#22D3EE" stroke-width="2"/>'
    b += text(164, 290, "Selection on white canvas + grid", 10, "#41557A", 600) + text(476, 290, "Selection on blackboard + grid", 10, "#41557A", 600)
    save("canvas-01-selection", W, H + 10, b, title="Selection: dashed cyan box, 4 corner resize handles, rotation handle 32 dp above")
    # polygon construction + long press
    b = '<rect x="16" y="40" width="296" height="230" rx="10" fill="#FFF"/>'
    pts = [(70, 220), (110, 110), (190, 90), (240, 170)]; ink = "#EF4444"
    b += f'<path d="M{" L".join(f"{x} {y}" for x, y in pts)}" fill="none" stroke="{ink}" stroke-width="6" stroke-linejoin="round" stroke-linecap="round"/>'
    b += f'<path d="M240 170L262 236" stroke="{ink}" stroke-opacity="0.5" stroke-width="3" stroke-dasharray="8 6" stroke-linecap="round"/>'
    b += f'<path d="M262 236L70 220" stroke="#22D3EE" stroke-opacity="0.6" stroke-width="1.5" stroke-dasharray="3 4"/>'
    b += f'<circle cx="70" cy="220" r="8" fill="none" stroke="#22D3EE" stroke-width="2"/>'
    for x, y in pts: b += f'<circle cx="{x}" cy="{y}" r="4" fill="#FFF" stroke="{ink}" stroke-width="2"/>'
    b += pen_tip(262, 236)
    b += text(164, 62, "Rubber-band segment: ink 50%, dash 8/6", 10, "#41557A", 600) + text(164, 290, "Polygon/polyline in progress; first vertex ring = close target", 10, "#41557A", 600)
    b += '<rect x="328" y="40" width="296" height="230" rx="10" fill="#FFF"/>'
    lx, ly, r = 476, 160, 20
    b += f'<circle cx="{lx}" cy="{ly}" r="{r}" fill="none" stroke="#22D3EE" stroke-opacity="0.2" stroke-width="3"/>'
    ang = 0.65 * 2 * math.pi; ex, ey = lx + r * math.sin(ang), ly - r * math.cos(ang)
    b += f'<path d="M{lx} {ly-r}A{r} {r} 0 1 1 {ex} {ey}" fill="none" stroke="#22D3EE" stroke-width="3" stroke-linecap="round"/>'
    b += pen_tip(lx, ly)
    b += text(476, 290, "Long-press ring: appears at 150 ms, full at 550 ms, then menu opens", 10, "#41557A", 600)
    save("canvas-02-path-and-longpress", W, H + 10, b, title="Path construction and long-press feedback")

def overview():
    from PIL import Image
    names = sorted(f for f in os.listdir(OUT) if f.startswith("menu-") and f[5:7].isdigit() and f[5:7] != "00")
    ims = [Image.open(os.path.join(OUT, n)) for n in names]
    w, h = ims[0].size; cols = 5
    sheet = Image.new("RGB", (w * cols, h * ((len(ims) + cols - 1) // cols)), (233, 238, 245))
    for i, im in enumerate(ims): sheet.paste(im, ((i % cols) * w, (i // cols) * h))
    sheet.resize((sheet.width // 2, sheet.height // 2), Image.LANCZOS).save(os.path.join(OUT, "drawdock-radial-menu-overview.png"))

if __name__ == "__main__":
    screens(); anatomy(); canvas(); overview(); print("mockups written to", OUT)
