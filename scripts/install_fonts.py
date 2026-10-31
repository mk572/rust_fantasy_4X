#!/usr/bin/env python3
"""Download the open-licence fonts listed in scripts/fonts.json into reference/fonts.

The fonts are not committed: the manifest is. Each download is checked against its pinned
sha256, and a font whose licence is not in the manifest's allowed list is refused, so only
fonts with no restrictions on use in the project can be installed. Standard library only.

    python3 scripts/install_fonts.py              install everything missing
    python3 scripts/install_fonts.py --list       show the fonts and their licences
    python3 scripts/install_fonts.py --only unifont jersey10
    python3 scripts/install_fonts.py --pin        write the sha256 of entries that have none
"""

import argparse
import hashlib
import io
import json
import sys
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "fonts.json"
DEFAULT_DEST = ROOT / "reference" / "fonts"
TIMEOUT_S = 120


def load_manifest() -> dict:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def licence_is_allowed(licence: str, allowed: list[str]) -> bool:
    """A choice ("A OR B WITH x") is allowed when one alternative is."""
    alternatives = [part.split(" WITH ")[0].strip() for part in licence.split(" OR ")]
    return any(alternative in allowed for alternative in alternatives)


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "f4x-install-fonts"})
    with urllib.request.urlopen(request, timeout=TIMEOUT_S) as response:
        return response.read()


def destinations(download: dict) -> list[str]:
    if "extract" in download:
        return list(download["extract"].values())
    return [download["dest"]]


def extract_members(data: bytes, members: dict[str, str]) -> dict[str, bytes]:
    """The wanted members of a .tar.gz, keyed by their destination."""
    files = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member, dest in members.items():
            handle = archive.extractfile(member)
            if handle is None:
                raise SystemExit(f"{member} is not a file in the archive")
            files[dest] = handle.read()
    return files


def install_download(download: dict, dest_dir: Path, pin: bool) -> bool:
    """Fetch one download; True when it changed the manifest (a pinned hash)."""
    wanted = destinations(download)
    pinned = download.get("sha256")
    if not pin and all((dest_dir / dest).exists() for dest in wanted):
        return False
    if pin and pinned:
        return False
    data = fetch(download["url"])
    digest = hashlib.sha256(data).hexdigest()
    if pinned and digest != pinned:
        raise SystemExit(f"{download['url']}: sha256 {digest} is not the pinned {pinned}")
    changed = False
    if not pinned:
        download["sha256"] = digest
        changed = True
    files = extract_members(data, download["extract"]) if "extract" in download else {
        download["dest"]: data
    }
    for dest, content in files.items():
        path = dest_dir / dest
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        print(f"  {dest} ({len(content)} bytes)")
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--list", action="store_true", help="show the fonts and licences")
    parser.add_argument("--only", nargs="+", metavar="ID", help="only these font ids")
    parser.add_argument("--pin", action="store_true", help="fill in missing sha256 values")
    parser.add_argument("--dest", type=Path, default=DEFAULT_DEST, help="install folder")
    args = parser.parse_args()

    manifest = load_manifest()
    fonts = manifest["fonts"]
    if args.only:
        unknown = set(args.only) - {font["id"] for font in fonts}
        if unknown:
            raise SystemExit(f"unknown font id: {', '.join(sorted(unknown))}")
        fonts = [font for font in fonts if font["id"] in args.only]
    for font in fonts:
        if not licence_is_allowed(font["licence"], manifest["allowed_licences"]):
            raise SystemExit(f"{font['id']}: licence {font['licence']} is not allowed")
    if args.list:
        for font in fonts:
            print(f"{font['id']:20} {font['licence']:12} {font['name']}: {font['role']}")
        return 0

    changed = False
    for font in fonts:
        print(font["id"])
        for download in font["downloads"]:
            changed |= install_download(download, args.dest, args.pin)
    if changed:
        MANIFEST.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        print("pinned new sha256 values in scripts/fonts.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
