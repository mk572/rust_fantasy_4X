#!/usr/bin/env python3
"""A pixel editor for small sprites and tiles, painted with the original's 256-colour palette.

Starts a local web server (127.0.0.1 only) that serves scripts/sprite_editor.html. A picture is
edited as palette indexes (index 0 is transparent) on a canvas of any size (for the city grid's
tiles 32 x 17), and saved as a plain RGBA PNG into a target folder that is chosen on the page
(a folder browser inside the project); the pictures to open and the sheets to cut are listed from a
separate source folder, chosen the same way but inside assets-local (the original's art). A sheet in the format of `f4x_extract::pack` (a PNG with an
`f4x-sheet` text chunk, such as crates/f4x-client/assets/combat/city_grid.sheet.png) can be cut
into one PNG per picture in the target folder, to be edited there. The palette is the base palette
of the original's FONTS.LBX (entry 2, 6-bit RGB expanded to 8 bits); indexes 224 to 255 are black in
it, the screens fill them. Needs Pillow (.venv).

    .venv/bin/python scripts/sprite_editor.py             then open http://127.0.0.1:8767
    .venv/bin/python scripts/sprite_editor.py --dir DIR --port 9000

--dir is the folder the page starts in (default crates/f4x-client/assets/combat). The server writes only
inside the project folder and reads only inside assets-local. A picture opened from a PNG takes, for each pixel, the
palette colour nearest to it; the page says how many pixels were not an exact match.
"""

import argparse
import io
import json
import re
import struct
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PAGE = Path(__file__).with_name("sprite_editor.html")
DEFAULT_LBX = ROOT / "reference/mom-classic/Master of Magic Official Release/FONTS.LBX"
DEFAULT_DIR = ROOT / "crates" / "f4x-client" / "assets" / "combat"
SOURCE_ROOT = ROOT / "assets-local"
PALETTE_ENTRY = 2
LARGEST = 512
FILE_NAME = re.compile(r"[A-Za-z0-9_.-]+\.png")


def read_palette(lbx: Path) -> list[tuple[int, int, int]]:
    """The 256 colours of the base palette: FONTS.LBX entry 2, 6-bit values widened to 8 bits."""
    data = lbx.read_bytes()
    count, magic = struct.unpack_from("<HI", data, 0)
    if magic != 0x0000FEAD or count <= PALETTE_ENTRY:
        raise ValueError(f"not the expected FONTS.LBX: {lbx}")
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    block = data[offsets[PALETTE_ENTRY] : offsets[PALETTE_ENTRY] + 768]
    return [
        tuple((v << 2) | (v >> 4) for v in (block[i] & 63, block[i + 1] & 63, block[i + 2] & 63))
        for i in range(0, 768, 3)
    ]


class Sprites:
    """What the page asks for: the palette, folders, pictures read and written as indexes."""

    def __init__(self, palette: list[tuple[int, int, int]]) -> None:
        self.palette = palette
        self.exact = {c: i for i, c in reversed(list(enumerate(palette))) if i}

    def inside(self, text: str, root: Path = ROOT) -> Path:
        """`text` as a path that is in `root` (the project, or assets-local for a source), or an error."""
        path = Path(text).resolve()
        if path != root and root not in path.parents:
            raise ValueError(f"outside {root}: {path}")
        return path

    def folder(self, text: str, root: Path = ROOT) -> dict:
        path = self.inside(text, root)
        if not path.is_dir():
            raise ValueError(f"not a folder: {path}")
        return {
            "path": str(path),
            "parent": str(path.parent) if path != root else None,
            "dirs": sorted(p.name for p in path.iterdir() if p.is_dir() and not p.name.startswith(".")),
            "pngs": sorted(p.name for p in path.glob("*.png") if not p.name.endswith(".sheet.png")),
            "sheets": sorted(p.name for p in path.glob("*.sheet.png")),
        }

    def nearest(self, colour: tuple[int, int, int]) -> int:
        return min(
            range(1, 256),
            key=lambda i: sum((a - b) ** 2 for a, b in zip(self.palette[i], colour)),
        )

    def read(self, text: str) -> dict:
        """A PNG of the source root as palette indexes (alpha under 128 is index 0) and how many are
        not exact."""
        image = Image.open(self.inside(text, SOURCE_ROOT)).convert("RGBA")
        if max(image.size) > LARGEST:
            raise ValueError(f"{image.size[0]}x{image.size[1]} is over {LARGEST} pixels")
        inexact, cache, pixels = 0, {}, []
        for r, g, b, a in image.getdata():
            if a < 128:
                pixels.append(0)
                continue
            if (r, g, b) not in cache:
                exact = self.exact.get((r, g, b))
                cache[(r, g, b)] = (exact, self.nearest((r, g, b))) if exact is None else (exact, exact)
            exact, index = cache[(r, g, b)]
            inexact += exact is None
            pixels.append(index)
        return {"w": image.width, "h": image.height, "pixels": pixels, "inexact": inexact}

    def write(self, edit: dict) -> str:
        folder = self.inside(edit["folder"])
        name = edit["name"]
        if not FILE_NAME.fullmatch(name) or name.endswith(".sheet.png"):
            raise ValueError(f"not a picture file name (letters, digits, _ - . and .png): {name}")
        width, height, pixels = edit["w"], edit["h"], edit["pixels"]
        if not (0 < width <= LARGEST and 0 < height <= LARGEST) or len(pixels) != width * height:
            raise ValueError(f"{len(pixels)} pixels do not fill {width}x{height}")
        if any(not 0 <= p < 256 for p in pixels):
            raise ValueError("a pixel is not a palette index")
        image = Image.new("RGBA", (width, height), (0, 0, 0, 0))
        image.putdata([(*self.palette[p], 255) if p else (0, 0, 0, 0) for p in pixels])
        if not folder.is_dir():
            raise ValueError(f"not a folder: {folder}")
        image.save(folder / name)
        return str(folder / name)

    def cut_sheet(self, sheet_text: str, folder_text: str) -> list[str]:
        """Cut a pack's sheet into one PNG per picture in `folder`; the names the sheet gives."""
        sheet, folder = self.inside(sheet_text, SOURCE_ROOT), self.inside(folder_text)
        image = Image.open(sheet).convert("RGBA")
        lines = image.info.get("f4x-sheet", "").splitlines()
        if not lines or not lines[0].startswith("cell "):
            raise ValueError(f"no f4x-sheet text in {sheet}")
        cell_w, cell_h = (int(v) for v in lines[0][5:].split("x"))
        columns = image.width // cell_w
        written = []
        for n, line in enumerate(lines[1:]):
            name, w, h = line.split("\t")
            left, top = (n % columns) * cell_w, (n // columns) * cell_h
            image.crop((left, top, left + int(w), top + int(h))).save(folder / name)
            written.append(name)
        return written


def make_handler(sprites: Sprites, start: Path, page_text: bytes):
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

        def do_GET(self) -> None:
            url = urlparse(self.path)
            query = {k: v[0] for k, v in parse_qs(url.query).items()}
            try:
                if url.path == "/":
                    self.reply(200, page_text, "text/html; charset=utf-8")
                elif url.path == "/api/start":
                    self.json(200, {"folder": str(start), "source": str(SOURCE_ROOT), "palette": sprites.palette})
                elif url.path == "/api/folder":
                    root = SOURCE_ROOT if query.get("scope") == "source" else ROOT
                    self.json(200, sprites.folder(query["path"], root))
                elif url.path == "/api/picture":
                    self.json(200, sprites.read(query["path"]))
                else:
                    self.reply(404, b"not found", "text/plain")
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def do_POST(self) -> None:
            try:
                size = int(self.headers.get("Content-Length", 0))
                body = json.loads(self.rfile.read(size))
                if self.path == "/api/save":
                    self.json(200, {"saved": sprites.write(body)})
                elif self.path == "/api/cut":
                    self.json(200, {"written": sprites.cut_sheet(body["sheet"], body["folder"])})
                else:
                    self.reply(404, b"not found", "text/plain")
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def log_message(self, *args) -> None:
            pass

    return Handler


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dir", type=Path, default=DEFAULT_DIR, help="the folder the page starts in")
    parser.add_argument("--lbx", type=Path, default=DEFAULT_LBX, help="the original's FONTS.LBX")
    parser.add_argument("--port", type=int, default=8767)
    args = parser.parse_args()
    sprites = Sprites(read_palette(args.lbx))
    start = sprites.inside(str(args.dir))
    if not SOURCE_ROOT.is_dir():
        raise SystemExit(f"no source root: {SOURCE_ROOT}")
    if not start.is_dir():
        raise SystemExit(f"not a folder: {start}")
    server = ThreadingHTTPServer(
        ("127.0.0.1", args.port), make_handler(sprites, start, PAGE.read_bytes())
    )
    print(f"sprite editor in {start}: http://127.0.0.1:{args.port}  (Ctrl+C to stop)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
