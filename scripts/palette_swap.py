#!/usr/bin/env python3
"""Recolour the wizard portraits by region (hair, clothes, backdrop), three variants each.

Needs the player's own extracted art (assets-local/art/wizards) and Pillow (.venv).
Output is local-only: assets-local/palette-swap/NNN_<a|b|c>.png plus a contact sheet.
"""
import argparse
import colorsys
import sys
from pathlib import Path

from PIL import Image, ImageDraw

W, H = 109, 104
ALL = (0, 0, 1, 1)

# Selector: (box as fractions x0,y0,x1,y1, kind, lo, hi, gain).
#   kind "grey": low-saturation, light pixels; "dark": very dark pixels;
#   kind "hue": hue range lo..hi degrees (wraps) with some saturation.
# `skip` is a box whose inscribed ellipse is left alone (the face), `smax` the saturation limit for "grey".
def grey(box=ALL, skip=None, smax=.22): return (box, "grey", 0, smax, 1.0, skip)
def dark(box, gain=1.8, skip=None): return (box, "dark", 0, 0, gain, skip)
def hue(lo, hi, box=ALL, skip=None): return (box, "hue", lo, hi, 1.0, skip)

BLUE, PURPLE, RED, GREEN, GOLD = (190, 265), (265, 335), (335, 15), (65, 170), (35, 65)

RECIPES = {
    0: dict(hair=[grey()], cloth=[hue(*BLUE, (0, .5, 1, 1))], bg=[]),
    1: dict(hair=[grey((0, 0, 1, .9), skip=(.25, .35, .75, .65))], cloth=[], bg=[hue(*PURPLE, (0, 0, 1, .5))]),
    2: dict(hair=[], cloth=[hue(*BLUE), hue(*RED, (.2, 0, .8, .35))], bg=[]),
    3: dict(hair=[dark((.3, .4, .65, .9))], cloth=[hue(*BLUE), hue(*RED, (0, 0, .3, 1))], bg=[hue(*RED, (.75, 0, 1, 1))]),
    4: dict(hair=[dark((.3, .55, .7, .85))], cloth=[grey((.05, .1, .95, .9))], bg=[hue(*BLUE, (0, 0, 1, .6))]),
    5: dict(hair=[grey((0, .1, 1, .95), smax=.09)], cloth=[hue(*GREEN, (.2, 0, .8, .3))], bg=[hue(*PURPLE, (0, 0, 1, .5)), hue(*RED, (0, 0, 1, .5))]),
    6: dict(hair=[], cloth=[hue(*PURPLE, (0, 0, 1, 1))], bg=[hue(*BLUE)]),
    7: dict(hair=[], cloth=[hue(*GREEN, (.15, .2, .85, .8))], bg=[hue(*RED)]),
    8: dict(hair=[dark((.2, .1, .8, .45), 3), dark((.35, .5, .65, .8), 3)], cloth=[hue(*RED, (0, .5, 1, 1))], bg=[]),
    9: dict(hair=[hue(*GOLD, (.1, .05, .9, .7), (.22, .2, .78, .68))], cloth=[hue(*GREEN, (.3, .6, .7, 1))], bg=[hue(*BLUE)]),
    10: dict(hair=[hue(*GOLD, (.1, 0, .9, .5), (.22, .22, .78, .66))], cloth=[hue(20, 50, (0, .6, 1, 1), (.22, .4, .78, .9))], bg=[hue(*BLUE), hue(*GREEN, (0, .3, 1, .7))]),
    11: dict(hair=[hue(20, 50, (.1, 0, .9, .3)), hue(20, 50, (0, .3, .28, .9)), hue(20, 50, (.72, .3, 1, .9))], cloth=[hue(*GREEN, (0, .6, 1, 1)), hue(170, 200, (0, .6, 1, 1))], bg=[hue(*BLUE)]),
    12: dict(hair=[], cloth=[hue(*RED, (.1, 0, .9, .5)), hue(*GREEN, (0, 0, 1, 1)), hue(*BLUE)], bg=[]),
    13: dict(hair=[grey((.1, 0, .9, .5), (.3, .4, .7, .6)), dark((.2, .05, .8, .5), skip=(.3, .4, .7, .6))], cloth=[hue(*PURPLE, (0, .3, 1, 1), (.25, .3, .75, .75))], bg=[]),
}

# (hair hue, hair saturation, cloth hue shift, backdrop hue shift)
VARIANTS = {"a": (30, .65, 120, 60), "b": (2, .75, 240, 180), "c": (255, .5, 60, 300)}


def in_hue(h, lo, hi):
    return lo <= h <= hi if lo <= hi else (h >= lo or h <= hi)


def matches(sel, x, y, h, s, v):
    (x0, y0, x1, y1), kind, lo, hi, _, skip = sel
    if not (x0 * W <= x < x1 * W and y0 * H <= y < y1 * H):
        return False
    if skip:  # an ellipse inscribed in the box, so the edge does not show
        cx, cy = (skip[0] + skip[2]) / 2 * W, (skip[1] + skip[3]) / 2 * H
        rx, ry = (skip[2] - skip[0]) / 2 * W, (skip[3] - skip[1]) / 2 * H
        if ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1:
            return False
    if kind == "grey":
        return s < hi and v > .35
    if kind == "dark":
        return v < .3
    return s > .22 and v > .12 and in_hue(h, lo, hi)


def recolour(im, recipe, variant):
    hair_h, hair_s, cloth_shift, bg_shift = variant
    px = im.load()
    out = im.copy()
    po = out.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            if a == 0:
                continue
            h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
            h *= 360
            for sel in recipe["hair"]:
                if matches(sel, x, y, h, s, v):
                    nh, ns, nv = hair_h, max(s, hair_s * min(1, v + .2)), min(1, v * sel[4])
                    break
            else:
                for part, shift in (("cloth", cloth_shift), ("bg", bg_shift)):
                    if any(matches(sel, x, y, h, s, v) for sel in recipe[part]):
                        nh, ns, nv = (h + shift) % 360, s, v
                        break
                else:
                    continue
            r, g, b = colorsys.hsv_to_rgb(nh / 360, ns, nv)
            po[x, y] = (round(r * 255), round(g * 255), round(b * 255), a)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--in-dir", default="assets-local/art/wizards")
    ap.add_argument("--out-dir", default="assets-local/palette-swap")
    ap.add_argument("--scale", type=int, default=2, help="contact sheet scale")
    args = ap.parse_args()
    src, dst = Path(args.in_dir), Path(args.out_dir)
    dst.mkdir(parents=True, exist_ok=True)
    cell, sc = (W * args.scale + 4, H * args.scale + 4), args.scale
    sheet = Image.new("RGBA", (cell[0] * 4, cell[1] * 14), (40, 40, 40, 255))
    for i, recipe in RECIPES.items():
        base = Image.open(src / f"{i:03d}_00.png").convert("RGBA")
        versions = [("orig", base)] + [(n, recolour(base, recipe, v)) for n, v in VARIANTS.items()]
        for col, (name, im) in enumerate(versions):
            if name != "orig":
                im.save(dst / f"{i:03d}_{name}.png")
            big = im.resize((W * sc, H * sc), Image.NEAREST)
            sheet.paste(big, (col * cell[0], i * cell[1]), big)
    sheet.save(dst / "contact_sheet.png")
    print(f"wrote {len(RECIPES) * len(VARIANTS)} portraits and contact_sheet.png to {dst}")


if __name__ == "__main__":
    sys.exit(main())
