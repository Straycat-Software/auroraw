#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Builds Auroraw's icon set: the font the interface draws its icons with, and the same icons as SVG files.

The icons are defined here, as outlines on a 1000 by 1000 grid (y up), and nowhere else: `AurorawIcons.ttf`
(what `AppIcon.qml` sets a private-use character in), `crates/ui/qml/Icons.qml` (the names and their characters)
and `crates/ui/assets/icons/*.svg` (what the design system shows) are all written from these outlines. Run it after
changing one, then commit the outputs:

    python3 -m venv /tmp/icons && /tmp/icons/bin/pip install fonttools
    /tmp/icons/bin/python tools/icons/build_icons.py

An icon is drawn to be set at 11 to 18 pixels, in one colour, so its strokes are heavy (130 to 150 units, about
1.5 to 2.5 pixels there) and its shapes are simple. The glyphs are 1000 units wide and sit in a line box of
1000 units (ascent 800, descent 200), so a `font.pixelSize` of N makes an icon N pixels square.
"""
import math
import pathlib
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

ROOT = pathlib.Path(__file__).resolve().parents[2]
FONT_PATH = ROOT / "crates/ui/assets/fonts/AurorawIcons.ttf"
SVG_DIR = ROOT / "crates/ui/assets/icons"
QML_PATH = ROOT / "crates/ui/qml/Icons.qml"
FAMILY = "Auroraw Icons"
FIRST_CODE = 0xE000
DESCENT = 200  # the glyph box is [-200, 800]: the icon canvas 0..1000 is drawn 200 units down


# ---- outlines: lists of contours, a contour a list of (x, y) points -------------------------------------

def area(points):
    return sum(x1 * y2 - x2 * y1 for (x1, y1), (x2, y2) in zip(points, points[1:] + points[:1])) / 2


def solid(points):
    """A contour that fills (clockwise, the outer direction of a TrueType outline)."""
    return points if area(points) < 0 else points[::-1]


def hole(points):
    """A contour that cuts out of what it lies in (counter-clockwise)."""
    return points if area(points) > 0 else points[::-1]


def circle(cx, cy, r, n=20):
    return [(cx + r * math.cos(2 * math.pi * i / n), cy + r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def stroke(points, width, closed=False):
    """A polyline drawn `width` wide with round caps and joins: a quad per segment and a disc per vertex.
    The contours overlap and all fill (non-zero winding), so the union is the stroke."""
    out = []
    pts = list(points) + ([points[0]] if closed else [])
    for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
        length = math.hypot(x2 - x1, y2 - y1)
        nx, ny = -(y2 - y1) / length * width / 2, (x2 - x1) / length * width / 2
        out.append(solid([(x1 + nx, y1 + ny), (x2 + nx, y2 + ny), (x2 - nx, y2 - ny), (x1 - nx, y1 - ny)]))
    for x, y in points:
        out.append(solid(circle(x, y, width / 2)))
    return out


def rotate(contours, degrees, cx=500, cy=500):
    c, s = math.cos(math.radians(degrees)), math.sin(math.radians(degrees))
    return [[(cx + (x - cx) * c - (y - cy) * s, cy + (x - cx) * s + (y - cy) * c) for x, y in contour] for contour in contours]


def mirror(contours):
    return [[(1000 - x, y) for x, y in contour][::-1] for contour in contours]


def star_points(cx, cy, outer, inner):
    pts = []
    for i in range(10):
        r = outer if i % 2 == 0 else inner
        a = math.radians(90 + i * 36)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def scale_about(points, k, cx, cy):
    return [(cx + (x - cx) * k, cy + (y - cy) * k) for x, y in points]


# ---- the icons --------------------------------------------------------------------------------------------

STAR_R, STAR_r = 470, 215
STAR_CY = 500 - STAR_R * 0.0955  # the star's box, not its centre, is centred on the canvas


def star():
    return [solid(star_points(500, STAR_CY, STAR_R, STAR_r))]


def star_outline():
    outer = star_points(500, STAR_CY, STAR_R, STAR_r)
    return [solid(outer), hole(scale_about(outer, 0.56, 500, STAR_CY + 20))]


def check():
    return stroke([(190, 480), (410, 270), (820, 740)], 150)


def close():
    return stroke([(215, 215), (785, 785)], 150) + stroke([(215, 785), (785, 215)], 150)


def caret_down():
    return [solid([(215, 665), (785, 665), (500, 335)])]


def menu():
    return stroke([(170, 265), (830, 265)], 130) + stroke([(170, 500), (830, 500)], 130) + stroke([(170, 735), (830, 735)], 130)


def chevrons_right():
    one = [(215, 225), (480, 500), (215, 775)]
    two = [(x + 285, y) for x, y in one]
    return [c for c in stroke(one, 135) + stroke(two, 135)]


def minus():
    return stroke([(215, 500), (785, 500)], 140)


def series():
    """A stack of photographs: the front one whole, and two more behind it, each seen as the L of its top and right
    edges, offset up and to the right. Three frames say "several" at once, and at 13 pixels the edges still read."""
    front_w, front_h, step, edge = 520, 400, 110, 80
    x0, y0 = 0, 0

    def rect(a, b, c, d):
        return [(a, b), (c, b), (c, d), (a, d)]

    contours = [solid(rect(x0, y0, x0 + front_w, y0 + front_h))]
    for k in (1, 2):
        x1, y1 = x0 + front_w + k * step, y0 + front_h + k * step
        contours.append(solid(rect(x0 + k * step, y1 - edge, x1, y1)))  # the top edge
        contours.append(solid(rect(x1 - edge, y0 + k * step, x1, y1)))  # the right edge
    # centred on the canvas by the box of what is drawn
    xs = [x for c in contours for x, _ in c]
    ys = [y for c in contours for _, y in c]
    dx, dy = 500 - (min(xs) + max(xs)) / 2, 500 - (min(ys) + max(ys)) / 2
    return [[(x + dx, y + dy) for x, y in c] for c in contours]


ICONS = {
    "star": star(),
    "star-outline": star_outline(),
    "check": check(),
    "close": close(),
    "caret-down": caret_down(),
    "caret-up": rotate(caret_down(), 180),
    "caret-left": rotate(caret_down(), 270),
    "caret-right": rotate(caret_down(), 90),
    "menu": menu(),
    "chevrons-right": chevrons_right(),
    "chevrons-left": mirror(chevrons_right()),
    "minus": minus(),
    "series": series(),
}
NAMES = list(ICONS)


# ---- the outputs ------------------------------------------------------------------------------------------

def glyph(contours):
    pen = TTGlyphPen(None)
    for contour in contours:
        pts = [(round(x), round(y) - DESCENT) for x, y in contour]
        pen.moveTo(pts[0])
        for p in pts[1:]:
            pen.lineTo(p)
        pen.closePath()
    return pen.glyph()


def build_font():
    order = [".notdef"] + NAMES
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({FIRST_CODE + i: name for i, name in enumerate(NAMES)})
    glyphs = {".notdef": TTGlyphPen(None).glyph()}
    glyphs.update({name: glyph(ICONS[name]) for name in NAMES})
    builder.setupGlyf(glyphs)
    metrics = {}
    for name in order:
        g = builder.font["glyf"][name]
        g.recalcBounds(builder.font["glyf"])
        metrics[name] = (1000, getattr(g, "xMin", 0) if g.numberOfContours else 0)
    builder.setupHorizontalMetrics(metrics)
    builder.setupHorizontalHeader(ascent=800, descent=-DESCENT)
    builder.setupNameTable({
        "familyName": FAMILY, "styleName": "Regular",
        "copyright": "Copyright the Auroraw contributors. GPL-3.0-or-later.",
        "licenseDescription": "The icons of Auroraw, drawn for it: GPL-3.0-or-later, like the application.",
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-DESCENT, sTypoLineGap=0, usWinAscent=800, usWinDescent=DESCENT,
                     fsType=0, achVendID="AURW")
    builder.setupPost(isFixedPitch=1)
    # A build that does not depend on the day it runs.
    builder.font["head"].created = builder.font["head"].modified = 3786825600
    FONT_PATH.parent.mkdir(parents=True, exist_ok=True)
    builder.save(str(FONT_PATH))


def svg(contours):
    path = " ".join("M" + " L".join(f"{x:.0f} {1000 - y:.0f}" for x, y in contour) + " Z" for contour in contours)
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000" width="24" height="24">'
            f'<path fill="currentColor" fill-rule="nonzero" d="{path}"/></svg>\n')


def build_svgs():
    SVG_DIR.mkdir(parents=True, exist_ok=True)
    for name in NAMES:
        (SVG_DIR / f"{name}.svg").write_text(svg(ICONS[name]))


# The Unicode characters the interface used before it had icons, each with the icon that replaces it (`Icons.styled`
# does the substitution in a text that comes from elsewhere, such as the summary the engine writes).
LEGACY = {
    "\u2605": "star", "\u2606": "star-outline", "\u2714": "check", "\u2713": "check", "\u2716": "close", "\u2715": "close",
    "\u25BE": "caret-down", "\u25BC": "caret-down", "\u25B2": "caret-up", "\u25B8": "caret-right", "\u25B6": "caret-right",
    "\u25C0": "caret-left", "\u2630": "menu", "\u25A3": "series",
}


def write_qml():
    code_lines = ",\n".join(f'        "{name}": "\\u{code:04x}"' for name, code in codes().items())
    legacy_lines = ",\n".join(f'        "\\u{ord(ch):04x}": "{name}"' for ch, name in LEGACY.items())
    QML_PATH.write_text(f'''// SPDX-License-Identifier: GPL-3.0-or-later
pragma Singleton
import QtQuick

// The icons of the interface (D-137): a font of their own, `assets/fonts/AurorawIcons.ttf`, in which each icon is a
// character of the Unicode private-use area, so that an icon is set like text: its colour is `color`, its size the
// `font.pixelSize`, and it is as sharp as text on every platform and every rendering backend.
//
// GENERATED by `tools/icons/build_icons.py`, from the outlines defined there: change an icon or add one there, and
// run it; do not edit this file. `AppIcon.qml` is the item to draw one with.
QtObject {{
    property FontLoader loader: FontLoader {{ source: Qt.resolvedUrl("../assets/fonts/AurorawIcons.ttf") }}
    readonly property string family: loader.name
    readonly property var codes: ({{
{code_lines}
    }})
    readonly property var names: Object.keys(codes)

    // The character of icon `name`, or nothing for a name that is not one.
    function glyph(name) {{
        return codes[name] || ""
    }}

    // The Unicode characters the interface used before it had icons, and the icon that replaces each.
    readonly property var legacy: ({{
{legacy_lines}
    }})

    // `text` for a Label displayed as `Text.StyledText`, with each of those characters written as its icon:
    // for a sentence that comes from elsewhere (the summary the engine writes under the grid) and holds one.
    function styled(text) {{
        let out = ""
        for (const ch of String(text)) {{
            if (legacy[ch] !== undefined)
                out += "<font face=\\"" + family + "\\">" + glyph(legacy[ch]) + "</font>"
            else if (ch === "&")
                out += "&amp;"
            else if (ch === "<")
                out += "&lt;"
            else
                out += ch
        }}
        return out
    }}
}}
''')


def codes():
    """`name = U+E000` lines, what `Icons.qml` carries."""
    return {name: FIRST_CODE + i for i, name in enumerate(NAMES)}


if __name__ == "__main__":
    build_font()
    build_svgs()
    write_qml()
    for name, code in codes().items():
        print(f"{name:16} U+{code:04X}")
    print(f"wrote {FONT_PATH.relative_to(ROOT)}, {QML_PATH.relative_to(ROOT)} and {len(NAMES)} SVGs in {SVG_DIR.relative_to(ROOT)}", file=sys.stderr)
