"""Readers for the original game's `.lbx` files, shared by the scripts that read them.

`read_lbx` splits an archive into entries, `read_records` splits a data-table entry into its
records, `cstr` reads a zero-terminated string, `slug` and `ron_str` write ids and RON strings.
The text import that used to live here is `scripts/import_mom_text.py`.
"""

from __future__ import annotations

import re
import struct
from pathlib import Path


# --------------------------------------------------------------------------- #
# Reading the original files
# --------------------------------------------------------------------------- #


def read_lbx(path: Path) -> list[bytes]:
    """Return the entries of an LBX archive.

    Raises:
        ValueError: if the file does not have the LBX magic number.
    """
    data = path.read_bytes()
    count, magic = struct.unpack_from("<HH", data, 0)
    if magic != 0xFEAD:
        raise ValueError(f"not an LBX file (magic {magic:#06x}): {path}")
    offsets = struct.unpack_from(f"<{count + 1}I", data, 8)
    return [data[offsets[i] : offsets[i + 1]] for i in range(count)]


def read_records(entry: bytes) -> list[bytes]:
    """Split a data-table entry (`u16 count, u16 size, records...`) into records."""
    count, size = struct.unpack_from("<HH", entry, 0)
    return [entry[4 + i * size : 4 + (i + 1) * size] for i in range(count)]


def cstr(raw: bytes) -> str:
    return raw.split(b"\0")[0].decode("latin1")


def slug(name: str) -> str:
    """Lowercase `[a-z0-9_]` id as the content validator requires."""
    s = re.sub(r"[^a-z0-9]+", "_", name.lower().replace("'", ""))
    return s.strip("_")


def ron_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
