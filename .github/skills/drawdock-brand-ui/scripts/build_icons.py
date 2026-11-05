#!/usr/bin/env python3
"""DrawDock icon source of truth.

Generates, from the ICONS table below:
  assets/icons/svg/dd_<name>.svg                    24x24, stroke 2, currentColor
  assets/android/res/drawable/ic_dd_<name>.xml      Android VectorDrawable (white; tint at use site)
  assets/icons/drawdock-icon-sheet.png              labelled preview (needs cairosvg + Pillow)

Usage:  python3 scripts/build_icons.py            (run from the skill root)
Add an icon: append an entry to ICONS using only the helpers below (they all emit
<path> data so the VectorDrawable conversion stays lossless), then re-run.
"""
import os, sys, io

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SVG_DIR = os.path.join(ROOT, "assets/icons/svg")
VD_DIR = os.path.join(ROOT, "assets/android/res/drawable")

# ---------- primitives (all return (d, kind, stroke_width)) ----------
def P(d, sw=2.0):            return (d, "stroke", sw)
def F(d):                    return (d, "fill", 0)
def rect(x, y, w, h, r=0, sw=2.0):
    if r == 0:
        return P(f"M{x} {y}h{w}v{h}h{-w}z", sw)
    return P(f"M{x+r} {y}h{w-2*r}a{r} {r} 0 0 1 {r} {r}v{h-2*r}a{r} {r} 0 0 1 {-r} {r}"
             f"h{-(w-2*r)}a{r} {r} 0 0 1 {-r} {-r}v{-(h-2*r)}a{r} {r} 0 0 1 {r} {-r}z", sw)
def ell(cx, cy, rx, ry, sw=2.0):
    return P(f"M{cx-rx} {cy}a{rx} {ry} 0 1 0 {2*rx} 0a{rx} {ry} 0 1 0 {-2*rx} 0z", sw)
def circ(cx, cy, r, sw=2.0):  return ell(cx, cy, r, r, sw)
def dot(cx, cy, r=1.4):
    return F(f"M{cx-r} {cy}a{r} {r} 0 1 0 {2*r} 0a{r} {r} 0 1 0 {-2*r} 0z")

CORNERS = P("M3.5 8V4.5a1 1 0 0 1 1-1H8M16 3.5h3.5a1 1 0 0 1 1 1V8M20.5 16v3.5a1 1 0 0 1-1 1H16M8 20.5H4.5a1 1 0 0 1-1-1V16")
PEN = [P("M4 20l1.2-4.4L15.6 5.2a2.1 2.1 0 0 1 3 0l.2.2a2.1 2.1 0 0 1 0 3L8.4 18.8 4 20z"), P("M13.8 7l3.2 3.2")]
CURSOR = [P("M5 3.5l13 7.2-5.6 1.6 3.4 6.2-2.4 1.3-3.4-6.2L6 17.6z")]

# group, name, label, primitives
ICONS = [
 # --- radial root ---
 ("root", "pen", "Pen", PEN),
 ("root", "shapes", "Shapes", [rect(3, 3, 9.5, 9.5, 1.5), circ(16.25, 16.25, 4.75), P("M15 9.5l2.75-5.5 2.75 5.5z")]),
 ("root", "select_edit", "Select / Edit", [CORNERS, P("M9.5 8.5l7 3.6-2.9.8 1.8 3.2-1.4.8-1.8-3.2-2.2 2z")]),
 ("root", "style", "Style", [P("M12 3a9 9 0 1 0 0 18c1 0 1.7-.8 1.7-1.7 0-.5-.2-.9-.5-1.2a1.7 1.7 0 0 1 1.3-2.9H17a4 4 0 0 0 4-4c0-4.5-4-8.2-9-8.2z"), dot(7.5, 11.5), dot(9.8, 7.4), dot(14.5, 7.4)]),
 ("root", "board_grid", "Board / Grid", [rect(3, 3.5, 18, 14, 1.5), P("M9 3.5v14M15 3.5v14M3 10.5h18"), P("M7 21h10")]),
 ("root", "undo", "Undo", [P("M9 14L4 9l5-5"), P("M4 9h10.5a5.5 5.5 0 0 1 0 11H11")]),
 ("root", "redo", "Redo", [P("M15 14l5-5-5-5"), P("M20 9H9.5a5.5 5.5 0 0 0 0 11H13")]),
 ("root", "more", "More", [dot(5, 12, 1.8), dot(12, 12, 1.8), dot(19, 12, 1.8)]),
 # --- center buttons ---
 ("center", "close", "Close", [P("M6.5 6.5l11 11M17.5 6.5l-11 11")]),
 ("center", "back", "Back", [P("M19 12H5.5"), P("M11 6l-6 6 6 6")]),
 # --- Pen submenu ---
 ("pen", "freehand", "Freehand", [P("M3 16c2.5-5 4.5-7 6-6s-1 5 1 6 4-7 6-7 1.5 5 5 4")]),
 ("pen", "highlighter", "Highlighter", [P("M9.5 14.5l8.3-8.3a1.8 1.8 0 0 1 2.5 2.5L12 17z"), P("M9.5 14.5L7.5 19l4.5-2"), P("M3 21h7", 3.0)]),
 ("pen", "laser_pointer", "Laser pointer", [dot(12, 12, 3), P("M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1")]),
 # --- Shapes submenu ---
 ("shapes", "rectangle", "Rectangle", [rect(3.5, 5.5, 17, 13, 1.5)]),
 ("shapes", "circle", "Circle", [circ(12, 12, 8.5)]),
 ("shapes", "ellipse", "Ellipse", [ell(12, 12, 9.5, 6)]),
 ("shapes", "line", "Line", [P("M5.5 18.5l13-13"), dot(5, 19, 1.9), dot(19, 5, 1.9)]),
 ("shapes", "polygon", "Polygon", [P("M12 3.5l8.5 6.2-3.2 10H6.7l-3.2-10z")]),
 ("shapes", "polyline", "Polyline", [P("M3.5 18l5-9 5 6 4-9 3 3"), dot(3.5, 18), dot(8.5, 9), dot(13.5, 15), dot(17.5, 6), dot(20.5, 9)]),
 ("shapes", "arrow", "Arrow", [P("M5 19L19 5"), P("M10 5h9v9")]),
 ("shapes", "text", "Text", [P("M5 7V4.5h14V7"), P("M12 4.5v15"), P("M9 19.5h6")]),
 # --- Select / Edit submenu ---
 ("select", "select_move", "Select / Move", [P("M12 3v18M3 12h18"), P("M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3")]),
 ("select", "rotate", "Rotate", [P("M20 12a8 8 0 1 1-2.34-5.66"), P("M20 4v4.5h-4.5")]),
 ("select", "resize", "Resize", [P("M14 4h6v6M10 20H4v-6"), P("M20 4l-6.5 6.5M4 20l6.5-6.5")]),
 ("select", "mirror_horizontal", "Mirror horizontally", [dot(12, 3.5, 1.1), dot(12, 9.2, 1.1), dot(12, 14.8, 1.1), dot(12, 20.5, 1.1), F("M9.5 6.5V18h-6z"), P("M14.5 6.5V18h6z")]),
 ("select", "mirror_vertical", "Mirror vertically", [dot(3.5, 12, 1.1), dot(9.2, 12, 1.1), dot(14.8, 12, 1.1), dot(20.5, 12, 1.1), F("M17.5 9.5H6V3.5z"), P("M17.5 14.5H6v6z")]),
 ("select", "deselect", "Deselect", [CORNERS, P("M9.5 9.5l5 5M14.5 9.5l-5 5")]),
 # --- Style submenu ---
 ("style", "color", "Color", [P("M12 3.5c3.5 4 6 7.2 6 10a6 6 0 0 1-12 0c0-2.8 2.5-6 6-10z")]),
 ("style", "width", "Width", [P("M4 6h16", 1.0), P("M4 11.5h16", 2.2), P("M4 17.5h16", 3.6)]),
 ("width", "width_thin", "Thin", [P("M4 15c3-5 5-5 8-3s5 2 8-3", 1.25)]),
 ("width", "width_medium", "Medium", [P("M4 15c3-5 5-5 8-3s5 2 8-3", 2.5)]),
 ("width", "width_thick", "Thick", [P("M4 15c3-5 5-5 8-3s5 2 8-3", 4.5)]),
 # --- Board / Grid submenu ---
 ("board", "blackboard", "Blackboard", [rect(2.5, 3.5, 19, 13, 1.5), P("M6.5 8.5h6M6.5 11.5h4"), P("M8.5 21l1.8-4.5M15.5 21l-1.8-4.5")]),
 ("board", "grid", "Grid", [rect(3.5, 3.5, 17, 17, 2), P("M9.2 3.5v17M14.8 3.5v17M3.5 9.2h17M3.5 14.8h17")]),
 # --- More submenu ---
 ("more", "mode_pointer", "Pointer mode", CURSOR),
 ("more", "mode_draw", "Draw mode", PEN + [P("M18.5 15.5v4M16.5 17.5h4", 1.6)]),
 ("more", "finish_path", "Finish path", [P("M3 14.5l4-6 4.5 3.5 4-5"), dot(3, 14.5), dot(7, 8.5), dot(11.5, 12), dot(15.5, 7), P("M12.5 18l2.5 2.5 5.5-5.5")]),
 ("more", "cancel_path", "Cancel path", [P("M3 14.5l4-6 4.5 3.5 4-5"), dot(3, 14.5), dot(7, 8.5), dot(11.5, 12), dot(15.5, 7), P("M14.5 15.5l5 5M19.5 15.5l-5 5")]),
 ("more", "paste_image", "Paste image", [P("M8.5 4.5H7a2 2 0 0 0-2 2V19a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V6.5a2 2 0 0 0-2-2h-1.5"), rect(8.5, 2.75, 7, 3.5, 1), P("M8 18l3-3.5 2 2 1.5-1.5 2.5 3"), dot(10, 10.5, 1.2)]),
 ("more", "clear_all", "Clear all", [P("M4 7h16"), P("M9 7V4.5h6V7"), P("M6 7l1 12.5a1.5 1.5 0 0 0 1.5 1.5h7a1.5 1.5 0 0 0 1.5-1.5L18 7"), P("M10 11v6M14 11v6")]),
 ("more", "settings", "Settings", [circ(12, 12, 3), P("M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z")]),
 # --- outside the radial menu: toolbar / connection ---
 ("app", "pen_tools", "Pen tools button", [circ(12, 12, 9), P("M8 16l.6-2.2 5.2-5.2a1.1 1.1 0 0 1 1.6 1.6l-5.2 5.2z", 1.6)]),
 ("app", "monitor_select", "Monitor selector", [rect(2.5, 4, 19, 12.5, 2), P("M8.5 20.5h7M12 16.5v4"), P("M8.5 10.3l2.3 2.3 4.7-4.7")]),
 ("app", "multi_monitor", "Multiple monitors", [rect(2, 6, 11, 8, 1.5), rect(11, 3, 11, 8, 1.5), P("M5.5 18h4M7.5 14v4")]),
 ("app", "usb", "USB connection", [P("M12 21V4"), P("M9.5 6.5L12 4l2.5 2.5"), P("M12 15.5l-4.5-2.5V10"), P("M12 13l4.5-2.5V8.5"), circ(7.5, 8.8, 1.3), rect(15.3, 6.3, 2.4, 2.2, 0.3), dot(12, 20, 1.3)]),
 ("app", "wifi", "Wi-Fi connection", [P("M2.5 9a14 14 0 0 1 19 0"), P("M5.8 12.5a9 9 0 0 1 12.4 0"), P("M9 16a4.4 4.4 0 0 1 6 0"), dot(12, 19.3, 1.2)]),
 ("app", "connected", "Connected", [rect(2, 8, 7, 11, 1.5), rect(13, 4, 9, 7, 1.5), P("M17.5 11v2.5"), P("M9 13.5h8.5")]),
 ("app", "disconnected", "Disconnected", [P("M10 7.5l1.6-1.6a4 4 0 0 1 5.7 5.7L15.7 13.2"), P("M14 16.5l-1.6 1.6a4 4 0 0 1-5.7-5.7L8.3 10.8"), P("M4 4l2 2M20 20l-2-2M4 9h2M9 4v2M20 15h-2M15 20v-2")]),
]

def fmt(v):
    return ("%g" % v)

def svg(prims):
    out = []
    for d, kind, sw in prims:
        if kind == "fill":
            out.append(f'<path d="{d}" fill="currentColor" stroke="none"/>')
        elif sw != 2.0:
            out.append(f'<path d="{d}" stroke-width="{fmt(sw)}"/>')
        else:
            out.append(f'<path d="{d}"/>')
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" '
            'stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
            + "".join(out) + "</svg>\n")

def vd(prims):
    out = []
    for d, kind, sw in prims:
        if kind == "fill":
            out.append(f'    <path android:fillColor="#FFFFFFFF" android:pathData="{d}"/>')
        else:
            out.append(f'    <path android:fillColor="#00000000" android:strokeColor="#FFFFFFFF" '
                       f'android:strokeWidth="{fmt(sw)}" android:strokeLineCap="round" '
                       f'android:strokeLineJoin="round" android:pathData="{d}"/>')
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            '<!-- DrawDock icon. Generated by scripts/build_icons.py; do not edit by hand. Tint at use site. -->\n'
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="24dp" android:height="24dp"\n'
            '    android:viewportWidth="24" android:viewportHeight="24">\n'
            + "\n".join(out) + "\n</vector>\n")

def main():
    os.makedirs(SVG_DIR, exist_ok=True); os.makedirs(VD_DIR, exist_ok=True)
    for f in os.listdir(SVG_DIR):
        if f.startswith("dd_"): os.remove(os.path.join(SVG_DIR, f))
    for f in os.listdir(VD_DIR):
        if f.startswith("ic_dd_"): os.remove(os.path.join(VD_DIR, f))
    for g, name, label, prims in ICONS:
        open(os.path.join(SVG_DIR, f"dd_{name}.svg"), "w").write(svg(prims))
        open(os.path.join(VD_DIR, f"ic_dd_{name}.xml"), "w").write(vd(prims))
    print(f"wrote {len(ICONS)} icons")
    try:
        sheet()
    except ImportError as e:
        print("preview skipped:", e, file=sys.stderr)

def sheet():
    import cairosvg
    from PIL import Image, ImageDraw, ImageFont
    groups = []
    for g, *_ in ICONS:
        if g not in groups: groups.append(g)
    titles = {"root": "Radial root", "center": "Center buttons", "pen": "Pen", "shapes": "Shapes",
              "select": "Select / Edit", "style": "Style", "width": "Style > Width",
              "board": "Board / Grid", "more": "More", "app": "Outside the menu"}
    cell, cols, pad = 132, 8, 28
    rows = []
    for g in groups:
        items = [i for i in ICONS if i[0] == g]
        rows.append(("title", g)); rows += [("icons", items[i:i+cols]) for i in range(0, len(items), cols)]
    H = sum(44 if r[0] == "title" else cell for r in rows) + pad * 2
    im = Image.new("RGB", (cols * cell + pad * 2, H), (11, 27, 58)); d = ImageDraw.Draw(im)
    def font(n, s):
        for p in [f"/usr/share/fonts/truetype/lato/Lato-{n}.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]:
            if os.path.exists(p): return ImageFont.truetype(p, s)
        return ImageFont.load_default()
    ft, fl = font("Semibold", 20), font("Medium", 14)
    y = pad
    for kind, val in rows:
        if kind == "title":
            d.text((pad, y + 12), titles.get(val, val).upper(), font=ft, fill=(34, 211, 238)); y += 44; continue
        for i, (g, name, label, prims) in enumerate(val):
            x = pad + i * cell
            d.rounded_rectangle((x + 30, y + 6, x + 102, y + 78), radius=36, fill=(19, 40, 79))
            png = cairosvg.svg2png(bytestring=svg(prims).replace("currentColor", "#E6F0FF").encode(), output_width=48, output_height=48)
            ic = Image.open(io.BytesIO(png)); im.paste(ic, (x + 42, y + 18), ic)
            w = d.textlength(label, font=fl)
            if w > cell - 6: label = name.replace("_", " ")
            w = d.textlength(label, font=fl)
            d.text((x + (cell - w) / 2, y + 88), label, font=fl, fill=(220, 230, 245))
        y += cell
    im.save(os.path.join(ROOT, "assets/icons/drawdock-icon-sheet.png"))

if __name__ == "__main__":
    main()
