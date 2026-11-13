#!/usr/bin/env python3
"""A cutting tool for the neutral races' pictures (task 523).

Starts a local web server (127.0.0.1 only) that serves scripts/portrait_cutter.html. The page
loads a large source picture, shows a crop frame of the right proportions that you move and resize
by hand, previews the exact result, and saves it. The three pictures of a race are cut from the
same source: `portrait` (109x109, the diplomacy screen), `gem` (41x43, lists and the magic screen)
and `hall` (30x32, the hall of fame).

The sources are the pictures in `crates/f4x-client/assets/neutral/<race id>/source/` and each
`assets-local/neutral/<race id>/original.jpg`. A save writes the cut picture to
`crates/f4x-client/assets/neutral/<race id>/<kind>.png` (what the client embeds) and to
`assets-local/neutral/<race id>/<kind>_<w>x<h>.png` (what `scripts/neutral_art.py` installs, so a
later run of it does not bring the old crop back), keeps the first replaced picture as
`assets-local/neutral/<race id>/old_<kind>.png`, and remembers the frame in
`assets-local/neutral/<race id>/crops.json` so the next visit starts where you stopped. The gem and hall
frames follow the portrait's: the same centre and the same zoom (target pixels per source pixel), so
they show the middle of the portrait and crop the pixels around it, until you move one by hand.
Needs Pillow (.venv).

    .venv/bin/python scripts/portrait_cutter.py          then open http://127.0.0.1:8766
    .venv/bin/python scripts/portrait_cutter.py --port 9000

The client embeds the pictures: rebuild it to see a change.
"""

import argparse
import io
import json
import re
import shutil
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PAGE = Path(__file__).with_name("portrait_cutter.html")
INSTALLED = ROOT / "crates" / "f4x-client" / "assets" / "neutral"
LOCAL = ROOT / "assets-local" / "neutral"
RACE = re.compile(r"[a-z_]+")
SOURCE_SUFFIXES = {".png", ".jpg", ".jpeg"}
# The pictures the client embeds (`neutral_art.rs`): the size is fixed by where they are drawn.
KINDS = {
    "portrait": (109, 109),
    "gem": (41, 43),
    "hall": (30, 32),
}
FILTERS = {
    "box": Image.Resampling.BOX,
    "lanczos": Image.Resampling.LANCZOS,
    "bicubic": Image.Resampling.BICUBIC,
    "bilinear": Image.Resampling.BILINEAR,
    "nearest": Image.Resampling.NEAREST,
}


def source_pictures() -> list[dict]:
    """Every source picture: its id (the path under the project), race and size."""
    paths = sorted(
        p
        for p in INSTALLED.glob("*/source/*")
        if p.suffix.lower() in SOURCE_SUFFIXES and RACE.fullmatch(p.parent.parent.name)
    ) + sorted(LOCAL.glob("*/original.jpg"))
    found = []
    for path in paths:
        race = path.parent.parent.name if path.parent.name == "source" else path.parent.name
        with Image.open(path) as image:
            width, height = image.size
        found.append(
            {
                "id": str(path.relative_to(ROOT)),
                "race": race,
                "name": path.name,
                "width": width,
                "height": height,
            }
        )
    return found


def source_path(picture_id: str) -> Path:
    """The file of a source picture id; refuses anything that is not one of the listed sources."""
    if picture_id not in {p["id"] for p in source_pictures()}:
        raise ValueError(f"not a source picture: {picture_id}")
    return ROOT / picture_id


def race_of(picture_id: str) -> str:
    return next(p["race"] for p in source_pictures() if p["id"] == picture_id)


def crop(picture_id: str, frame: dict, kind: str) -> Image.Image:
    """The frame (x, y, w, h in source pixels) cut out and scaled to the size of `kind`."""
    size = KINDS[kind]
    with Image.open(source_path(picture_id)) as image:
        image = image.convert("RGBA")
    x, y, w, h = (float(frame[k]) for k in ("x", "y", "w", "h"))
    if w < 1 or h < 1 or x < 0 or y < 0 or x + w > image.width + 1e-6 or y + h > image.height + 1e-6:
        raise ValueError(f"the frame {x},{y} {w}x{h} is not inside the {image.width}x{image.height} picture")
    return image.resize(size, FILTERS[frame.get("filter", "box")], box=(x, y, x + w, y + h))


def frames_file(race: str) -> Path:
    return LOCAL / race / "crops.json"


def saved_frames(race: str) -> dict:
    path = frames_file(race)
    return json.loads(path.read_text()) if path.is_file() else {}


def save(picture_id: str, kind: str, frame: dict) -> list[str]:
    """Cut and install one picture of a race. Returns the files written."""
    race = race_of(picture_id)
    picture = crop(picture_id, frame, kind)
    width, height = KINDS[kind]
    installed = INSTALLED / race / f"{kind}.png"
    local = LOCAL / race / f"{kind}_{width}x{height}.png"
    local.parent.mkdir(parents=True, exist_ok=True)
    backup = local.parent / f"old_{kind}.png"
    if installed.is_file() and not backup.exists():
        shutil.copy2(installed, backup)
    installed.parent.mkdir(parents=True, exist_ok=True)
    picture.save(installed)
    picture.save(local)
    frames = saved_frames(race)
    frames.setdefault(picture_id, {})[kind] = {
        **{k: frame[k] for k in ("x", "y", "w", "h", "filter")},
        "manual": bool(frame.get("manual", False)),
    }
    frames_file(race).write_text(json.dumps(frames, indent=2) + "\n")
    return [str(installed.relative_to(ROOT)), str(local.relative_to(ROOT))]


def make_handler():
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

        def query(self) -> dict:
            return {k: v[0] for k, v in parse_qs(urlparse(self.path).query).items()}

        def do_GET(self) -> None:
            path, query = urlparse(self.path).path, self.query()
            try:
                if path == "/":
                    self.reply(200, PAGE.read_bytes(), "text/html; charset=utf-8")
                elif path == "/api/pictures":
                    self.json(200, {"pictures": source_pictures(), "kinds": KINDS, "filters": list(FILTERS)})
                elif path == "/api/frames":
                    self.json(200, saved_frames(race_of(query["id"])).get(query["id"], {}))
                elif path == "/source":
                    file = source_path(query["id"])
                    kind = "image/png" if file.suffix.lower() == ".png" else "image/jpeg"
                    self.reply(200, file.read_bytes(), kind)
                elif path == "/crop":
                    out = io.BytesIO()
                    crop(query["id"], query, query["kind"]).save(out, "PNG")
                    self.reply(200, out.getvalue(), "image/png")
                elif path == "/installed":
                    file = INSTALLED / race_of(query["id"]) / f"{query['kind']}.png"
                    if query["kind"] not in KINDS or not file.is_file():
                        self.reply(404, b"none", "text/plain")
                    else:
                        self.reply(200, file.read_bytes(), "image/png")
                else:
                    self.reply(404, b"not found", "text/plain")
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def do_POST(self) -> None:
            try:
                size = int(self.headers.get("Content-Length", 0))
                body = json.loads(self.rfile.read(size))
                if body["kind"] not in KINDS:
                    raise ValueError(f"not a picture kind: {body['kind']}")
                self.json(200, {"written": save(body["id"], body["kind"], body)})
            except (OSError, ValueError, KeyError) as error:
                self.json(400, {"error": f"{type(error).__name__}: {error}"})

        def log_message(self, *args) -> None:
            pass

    return Handler


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=8766)
    args = parser.parse_args()
    if not source_pictures():
        raise SystemExit(f"no source pictures in {INSTALLED}/<race>/source/ or {LOCAL}/<race>/original.jpg")
    server = ThreadingHTTPServer(("127.0.0.1", args.port), make_handler())
    print(f"portrait cutter: http://127.0.0.1:{args.port}  (Ctrl+C to stop)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
