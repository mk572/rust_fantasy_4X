#!/usr/bin/env python3
"""A pixel editor for the BMFont files that `f4x-tools extract-art` writes.

Starts a local web server (127.0.0.1 only) that serves scripts/font_editor.html and reads and
writes the `.fnt` and `.png` pairs in assets-local/art/fonts/bmfont. A glyph is edited in the
first colour set; on save the pixels are copied into every stacked colour set, recoloured by the
mapping the file already has between the first set and each other set. The first save of a font
keeps the original beside it as `style2.png.orig` and `style2.fnt.orig`. Needs Pillow (.venv).

    .venv/bin/python scripts/font_editor.py              then open http://127.0.0.1:8765
    .venv/bin/python scripts/font_editor.py --dir DIR --port 9000

Re-running `extract-art` overwrites the files: keep the edits you want by copying the `.fnt` and
`.png` out, or port them into `crates/f4x-tools/src/accent.rs`.
"""

import argparse
import json
import re
import shutil
import sys
from collections import Counter
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PAGE = Path(__file__).with_name("font_editor.html")
NAME = re.compile(r"style\d+")
CHAR_FIELDS = ("id", "x", "y", "width", "height", "xoffset", "yoffset", "xadvance", "page", "chnl")


class Font:
    """One BMFont: its header lines, glyph table and atlas."""

    def __init__(self, folder: Path, name: str) -> None:
        if not NAME.fullmatch(name):
            raise ValueError(f"not a font name: {name}")
        self.fnt, self.png = folder / f"{name}.fnt", folder / f"{name}.png"
        self.lines = self.fnt.read_text(encoding="utf-8").splitlines()
        self.glyphs = [
            {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", line)}
            for line in self.lines
            if line.startswith("char ")
        ]
        common = dict(re.findall(r"(\w+)=(\d+)", next(l for l in self.lines if l.startswith("common"))))
        self.line_height, self.base = int(common["lineHeight"]), int(common["base"])
        self.set_height = int(common["scaleH"])
        self.atlas = Image.open(self.png).convert("RGBA")
        self.sets = self.atlas.height // self.set_height
        self.cell = (self._pitch("x") - 1, self._pitch("y") - 1)

    def _pitch(self, axis: str) -> int:
        """The distance between neighbouring glyph cells on an axis."""
        values = sorted({g[axis] for g in self.glyphs if g["y"] < self.set_height})
        steps = [b - a for a, b in zip(values, values[1:])]
        return min(steps) if steps else 1

    def info(self) -> dict:
        return {
            "lineHeight": self.line_height,
            "base": self.base,
            "sets": self.sets,
            "cell": self.cell,
            "glyphs": self.glyphs,
        }

    def colour_maps(self) -> list[dict[tuple, tuple]]:
        """For each set, the colour each first-set colour has there (read from the atlas)."""
        votes: list[dict[tuple, Counter]] = [{} for _ in range(self.sets)]
        px = self.atlas.load()
        for g in self.glyphs:
            for dy in range(g["height"]):
                for dx in range(g["width"]):
                    first = px[g["x"] + dx, g["y"] + dy]
                    if first[3] == 0:
                        continue
                    for k in range(self.sets):
                        there = px[g["x"] + dx, g["y"] + dy + k * self.set_height]
                        votes[k].setdefault(first, Counter())[there] += 1
        return [{c: n.most_common(1)[0][0] for c, n in v.items()} for v in votes]

    def save_glyph(self, edit: dict) -> None:
        glyph = next((g for g in self.glyphs if g["id"] == edit["id"]), None)
        if glyph is None:
            raise ValueError(f"no glyph {edit['id']}")
        width, height = edit["width"], edit["height"]
        if not (0 < width <= self.cell[0] and 0 < height <= self.cell[1]):
            raise ValueError(f"size {width}x{height} does not fit the {self.cell} cell")
        pixels = [tuple(int(p[i : i + 2], 16) for i in (0, 2, 4, 6)) for p in edit["pixels"]]
        if len(pixels) != width * height:
            raise ValueError("pixel count does not match the size")
        maps = self.colour_maps()
        unknown = {p for p in pixels if p[3] and p not in maps[0]}
        if unknown:
            raise ValueError(f"colours not in the font: {sorted(unknown)[:3]}")
        for k in range(self.sets):
            top = glyph["y"] + k * self.set_height
            for dy in range(self.cell[1]):
                for dx in range(self.cell[0]):
                    self.atlas.putpixel((glyph["x"] + dx, top + dy), (0, 0, 0, 0))
            for i, p in enumerate(pixels):
                if p[3]:
                    self.atlas.putpixel(
                        (glyph["x"] + i % width, top + i // width), maps[k].get(p, p)
                    )
        glyph.update(
            width=width, height=height, yoffset=edit["yoffset"], xadvance=edit["xadvance"]
        )
        self._write()

    def _write(self) -> None:
        for path in (self.fnt, self.png):
            backup = path.with_name(path.name + ".orig")
            if not backup.exists():
                shutil.copy2(path, backup)
        chars = iter(self.glyphs)
        lines = [
            " ".join(["char"] + [f"{f}={g[f]}" for f in CHAR_FIELDS])
            if line.startswith("char ")
            else line
            for line, g in ((line, next(chars) if line.startswith("char ") else None) for line in self.lines)
        ]
        self.fnt.write_text("\n".join(lines) + "\n", encoding="utf-8")
        self.atlas.save(self.png)


def make_handler(folder: Path):
    class Handler(BaseHTTPRequestHandler):
        def reply(self, status: int, body: bytes, kind: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", kind)
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def json(self, status: int, value) -> None:
            self.reply(status, json.dumps(value).encode(), "application/json")

        def font(self) -> Font:
            return Font(folder, parse_qs(urlparse(self.path).query)["name"][0])

        def do_GET(self) -> None:
            path = urlparse(self.path).path
            try:
                if path == "/":
                    self.reply(200, PAGE.read_bytes(), "text/html; charset=utf-8")
                elif path == "/api/fonts":
                    self.json(200, sorted(p.stem for p in folder.glob("style*.fnt")))
                elif path == "/api/font":
                    self.json(200, self.font().info())
                elif path == "/atlas.png":
                    self.reply(200, self.font().png.read_bytes(), "image/png")
                else:
                    self.reply(404, b"not found", "text/plain")
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def do_POST(self) -> None:
            try:
                size = int(self.headers.get("Content-Length", 0))
                self.font().save_glyph(json.loads(self.rfile.read(size)))
                self.json(200, {"ok": True})
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def log_message(self, *args) -> None:
            pass

    return Handler


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dir", type=Path, default=ROOT / "assets-local" / "art" / "fonts" / "bmfont")
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    if not any(args.dir.glob("style*.fnt")):
        raise SystemExit(f"no style*.fnt in {args.dir}: run f4x-tools extract-art first")
    server = ThreadingHTTPServer(("127.0.0.1", args.port), make_handler(args.dir))
    print(f"font editor for {args.dir}: http://127.0.0.1:{args.port}  (Ctrl+C to stop)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
