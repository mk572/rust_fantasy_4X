#!/usr/bin/env python3
"""Rank the installed free fonts by how closely their pixels match the original's font styles.

For each style of the original (assets-local/art/fonts/bmfont/style{N}.fnt, written by
`f4x-tools extract-art`) and each TrueType or OpenType font in reference/fonts (installed by
scripts/install_fonts.py), the font is drawn without anti-aliasing at a range of sizes. A glyph
is scored by the overlap of its pixels with the original glyph (intersection over union), after
the best shared shift of the whole font. The score of a font is the mean over the printable
ASCII glyphs; 1.0 would be a pixel-exact copy. Needs Pillow (.venv).

    .venv/bin/python scripts/font_match.py                  every style, top 5 each
    .venv/bin/python scripts/font_match.py --style 2 --top 10
    .venv/bin/python scripts/font_match.py --sizes 6 24
    .venv/bin/python scripts/font_match.py --style 5 --only jacquard
"""

import argparse
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
GLYPH_FIRST, GLYPH_LAST = 33, 126
BASELINE_ROW = 60
CANVAS = (120, 120)
SHIFT_X = range(-1, 2)

Pixels = frozenset[tuple[int, int]]


def parse_fnt(path: Path) -> dict[int, tuple[int, int, int, int]]:
    """Char id -> (x, y, width, height) of the first grid."""
    glyphs = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("char "):
            fields = dict(re.findall(r"(\w+)=(-?\d+)", line))
            glyphs[int(fields["id"])] = tuple(int(fields[k]) for k in ("x", "y", "width", "height"))
    return glyphs


def original_style(art_dir: Path, style: int) -> dict[int, Pixels]:
    """The original's glyphs as sets of (x, y) from the glyph's top left."""
    atlas = Image.open(art_dir / f"style{style}.png").convert("RGBA")
    result = {}
    for code, (x, y, width, height) in parse_fnt(art_dir / f"style{style}.fnt").items():
        if GLYPH_FIRST <= code <= GLYPH_LAST:
            result[code] = frozenset(
                (col, row)
                for row in range(height)
                for col in range(width)
                if atlas.getpixel((x + col, y + row))[3] > 0
            )
    return result


def candidate_glyphs(font: ImageFont.FreeTypeFont) -> dict[int, Pixels]:
    """Glyphs drawn once, as (x, y) from the pen position and the baseline (y up is negative)."""
    glyphs = {}
    for code in range(GLYPH_FIRST, GLYPH_LAST + 1):
        image = Image.new("1", CANVAS, 0)
        draw = ImageDraw.Draw(image)
        draw.fontmode = "1"
        draw.text((20, BASELINE_ROW), chr(code), font=font, fill=1, anchor="ls")
        pixels = image.load()
        glyphs[code] = frozenset(
            (x - 20, y - BASELINE_ROW)
            for y in range(CANVAS[1])
            for x in range(CANVAS[0])
            if pixels[x, y]
        )
    return glyphs


def overlap(original: Pixels, candidate: Pixels) -> float:
    union = len(original | candidate)
    return len(original & candidate) / union if union else 1.0


def best_score(
    original: dict[int, Pixels], candidate: dict[int, Pixels], height: int
) -> tuple[float, int, int]:
    """The best mean overlap over shifts: (score, baseline row of the original, x shift)."""
    best = (-1.0, 0, 0)
    for baseline in range(height // 2, height + 3):
        for dx in SHIFT_X:
            total = 0.0
            for code, pixels in original.items():
                moved = frozenset((x + dx, y + baseline) for x, y in candidate[code])
                total += overlap(pixels, moved)
            score = total / len(original)
            if score > best[0]:
                best = (score, baseline, dx)
    return best


def weight_variants(path: Path, size: int) -> list[tuple[str, ImageFont.FreeTypeFont]]:
    """The font, and for a variable font its lightest, default and heaviest weight."""
    font = ImageFont.truetype(str(path), size)
    try:
        axes = font.get_variation_axes()
    except (OSError, AttributeError):
        return [("", font)]
    weight = next((i for i, a in enumerate(axes) if b"eight" in bytes(a["name"])), None)
    if weight is None:
        return [("", font)]
    variants = []
    for label, key in (("light", "minimum"), ("regular", "default"), ("bold", "maximum")):
        variant = ImageFont.truetype(str(path), size)
        values = [a["default"] for a in axes]
        values[weight] = axes[weight][key]
        variant.set_variation_by_axes(values)
        variants.append((f" {label}", variant))
    return variants


def font_files(fonts_dir: Path) -> list[Path]:
    return sorted(
        path for path in fonts_dir.rglob("*") if path.suffix.lower() in (".ttf", ".otf")
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--style", type=int, help="only this style (0 to 7)")
    parser.add_argument("--top", type=int, default=5, help="rows per style")
    parser.add_argument("--sizes", type=int, nargs=2, default=(5, 24), metavar=("MIN", "MAX"))
    parser.add_argument("--only", metavar="TEXT", help="only font files whose path contains TEXT")
    parser.add_argument("--fonts", type=Path, default=ROOT / "reference" / "fonts")
    parser.add_argument(
        "--art", type=Path, default=ROOT / "assets-local" / "art" / "fonts" / "bmfont"
    )
    args = parser.parse_args()

    styles = [args.style] if args.style is not None else range(8)
    originals = {}
    for style in styles:
        if not (args.art / f"style{style}.fnt").exists():
            raise SystemExit(f"no {args.art}/style{style}.fnt: run f4x-tools extract-art first")
        originals[style] = original_style(args.art, style)
    files = [f for f in font_files(args.fonts) if not args.only or args.only in str(f)]
    if not files:
        raise SystemExit(f"no fonts in {args.fonts}: run scripts/install_fonts.py first")

    results = {style: [] for style in styles}
    low, high = args.sizes
    for path in files:
        for size in range(low, high + 1):
            try:
                variants = weight_variants(path, size)
            except OSError:
                break
            for label, font in variants:
                candidate = candidate_glyphs(font)
                for style in styles:
                    height = max(y for pixels in originals[style].values() for _, y in pixels) + 1
                    score, baseline, dx = best_score(originals[style], candidate, height)
                    name = f"{path.relative_to(args.fonts)}{label}"
                    results[style].append((score, name, size, baseline, dx))
        print(f"scored {path.name}", file=sys.stderr)

    for style in styles:
        print(f"\nstyle {style}: {len(originals[style])} glyphs")
        print("  score  font (size)  [baseline row, x shift]")
        for score, path, size, baseline, dx in sorted(results[style], reverse=True)[: args.top]:
            print(f"  {score:.3f}  {path} ({size})  [{baseline}, {dx}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
