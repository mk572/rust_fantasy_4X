#!/usr/bin/env python3
"""List every Set_Font_Colors_15 call of the original (as ReMoM's C source has them), per screen.

Set_Font_Colors_15(style, colors) fills the custom colour set 15 with palette indexes and selects
it. The script reads each function's code in order, keeps the colour arrays it fills (literal
initialisers, `a[i] = N;` and counted `for` loops; named tables like COL_HLP_Text come from their
definitions) and the last Set_Outline_Color before the call.
Usage: scripts/font_colors_by_screen.py [REMOM_DIR] [OUT.tsv]
Default input: ~/projects/ReMoM. Default output: docs/font_colors_by_screen.tsv
Columns: file (the screen's source file), function, line, style (font style number), outline
(Set_Outline_Color index, blank if none yet), colours (palette indexes of set entries 0..15 as
assigned, "." for an entry the code did not set), hex (the base palette's colour for each index
below 224; "screen" for 224 and up, see palette_screens.tsv), conditional (yes when the call is in
an if/else/loop: the values are read straight through as written), note (an array the script
could not resolve).
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REMOM = Path.home() / "projects/ReMoM"
CALL = re.compile(r"Set_Font_Colors_15\(\s*([^,]+?)\s*,\s*(.+?)\s*\)\s*;")
FUNC = re.compile(r"^[A-Za-z_][\w \*]*?\b(\w+)\s*\([^;]*\)\s*$")
FOR = re.compile(r"for\s*\(\s*(\w+)\s*=\s*(\d+)\s*;\s*\1\s*<\s*(\w+)\s*;\s*\1\+\+\s*\)")
CONSTANTS = {"NUM_PLAYERS": 6}
ASSIGN = re.compile(r"^\s*(\w+)\[\s*(\w+)\s*\]\s*=\s*([^;]+?)\s*;")
OUTLINE = re.compile(r"Set_Outline_Color\(\s*(\w+)\s*\)")
GLOBAL = re.compile(r"^\s*(?:static\s+)?(?:const\s+)?u?int\d*_t\s+(\w+)\s*\[[^\]]*\]\s*=\s*\{([^}]*)\}", re.M)


def number(text: str):
    text = text.strip()
    try:
        return int(text, 0)
    except ValueError:
        return text


def initialiser(body: str) -> dict:
    return {i: number(v) for i, v in enumerate(v for v in body.replace("\n", " ").split(",") if v.strip())}


GLOBAL_2D = re.compile(r"^\s*(?:static\s+)?(?:const\s+)?u?int\d*_t\s+(\w+)\s*\[[^\]]*\]\s*\[[^\]]*\]\s*=\s*\{(.*?)\n\s*\};", re.M | re.S)


def global_tables(src_dirs) -> dict:
    """Named colour tables; a two-dimensional one is stored as NAME[k] for each row k."""
    tables = {}
    for d in src_dirs:
        for path in d.glob("*.c"):
            text = path.read_text(errors="replace")
            for name, body in GLOBAL.findall(text):
                tables.setdefault(name, initialiser(body))
            for name, body in GLOBAL_2D.findall(text):
                for k, row in enumerate(re.findall(r"\{([^{}]*)\}", body)):
                    tables.setdefault(f"{name}[{k}]", initialiser(row))
    return tables


def run(lines, env, loop=None):
    """Execute assignments of a code block, expanding counted for loops."""
    i = 0
    while i < len(lines):
        line = re.sub(r"//.*", "", lines[i])
        m = FOR.search(line)
        if m:
            var, lo = m.group(1), int(m.group(2))
            bound = m.group(3)
            hi = int(bound) if bound.isdigit() else CONSTANTS.get(bound)
            unknown = hi is None
            if unknown:  # a bound that is a variable: the entries it fills are known only at run time
                hi = 16
            rest = line[m.end():].strip()
            if rest and rest != "{":
                body, i = [rest], i
            else:
                depth, body, i = 0, [], i + 1
                if rest != "{" and i < len(lines) and lines[i].strip() == "{":
                    i += 1
                depth = 1 if rest == "{" or lines[i - 1].strip() == "{" else 0
                while i < len(lines):
                    text = re.sub(r"//.*", "", lines[i])
                    depth += text.count("{") - text.count("}")
                    if depth <= 0:
                        break
                    body.append(text)
                    i += 1
                if depth == 0 and not body:
                    body = [re.sub(r"//.*", "", lines[i])]
            for n in range(lo, hi):
                run([re.sub(rf"\[\s*{var}\s*\]", f"[{n}]", b) for b in body], env)
                if unknown:
                    for name in {a.group(1) for a in (ASSIGN.match(b) for b in body) if a}:
                        env.setdefault(name, {})[n] = "?"
        else:
            m = ASSIGN.match(line)
            if m and m.group(2).isdigit():
                env.setdefault(m.group(1), {})[int(m.group(2))] = number(m.group(3))
            m = re.match(r"^\s*(\w+)\s*=\s*(\w+)\s*;", line)
            if m and m.group(2) in env:
                env[m.group(1)] = env[m.group(2)]
            m = re.match(r"^\s*(?:u?int\d*_t)\s+(\w+)\s*\[[^\]]*\]\s*=\s*\{([^}]*)\}", line)
            if m:
                env[m.group(1)] = initialiser(m.group(2))
        i += 1


def functions(text: str):
    lines = text.split("\n")
    start = None
    for n, line in enumerate(lines):
        if line and not line[0].isspace() and line.rstrip().endswith(")") and FUNC.match(line) \
                and n + 1 < len(lines) and lines[n + 1].strip() == "{":
            if start is not None:
                yield start[1], start[0], lines[start[0] : n]
            start = (n, FUNC.match(line).group(1))
    if start is not None:
        yield start[1], start[0], lines[start[0] :]


def base_palette(game: Path):
    import struct
    fonts = next(p for p in game.iterdir() if p.name.upper() == "FONTS.LBX")
    data = fonts.read_bytes()
    off = struct.unpack_from("<I", data, 8 + 8)[0]
    return data[off : off + 768]


def hexes(values, pal):
    out = []
    for v in values:
        if isinstance(v, int) and v < 224:
            r, g, b = (c & 0x3F for c in pal[3 * v : 3 * v + 3])
            out.append("#%02X%02X%02X" % tuple((c << 2) | (c >> 4) for c in (r, g, b)))
        elif isinstance(v, int):
            out.append("screen")
        elif v in ("?", "."):
            out.append(v)
        else:
            out.append(str(v))
    return out


def main() -> int:
    remom = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_REMOM
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/font_colors_by_screen.tsv"
    game = ROOT / "reference/mom-classic/Master of Magic Official Release"
    pal = base_palette(game)
    dirs = [remom / "MoM/src", remom / "MoX/src"]
    tables = global_tables(dirs)
    rows = ["file\tfunction\tline\tstyle\toutline\tcolours\thex\tconditional\tnote"]
    for d in dirs:
        for path in sorted(d.glob("*.c")):
            text = path.read_text(errors="replace")
            if "Set_Font_Colors_15" not in text:
                continue
            for name, first, body in functions(text):
                rows.extend(replay(path.name, name, first, body, tables, pal))
    out.write_text("\n".join(rows) + "\n")
    print(f"wrote {out} ({len(rows) - 1} rows)")
    return 0


def replay(fname, name, first, body, tables, pal):
    """Rows for the calls of one function, each with the colour arrays as they stood at the call."""
    env = {k: dict(v) for k, v in tables.items()}
    rows, outline, depth, pending = [], "", 0, []
    for n, raw in enumerate(body):
        line = re.sub(r"//.*", "", raw)
        m = CALL.search(line)
        if m:
            run(pending, env)
            pending = []
            style, expr = m.group(1), m.group(2).lstrip("&").replace("[0]", "")
            table = re.match(r"^(\w+)\[(\w+)\]$", expr)
            if table and f"{table.group(1)}[0]" in env:
                targets = [(f"{table.group(1)}[{k}]", f"{table.group(2)}={k}") for k in range(8)
                           if f"{table.group(1)}[{k}]" in env]
            else:
                targets = [(expr, "")]
            for arg, which in targets:
                values = env.get(arg)
                note = which or ("" if values is not None else f"unresolved: {m.group(2)}")
                colours = [(values or {}).get(k, ".") for k in range(16)]
                while colours and colours[-1] == ".":
                    colours.pop()
                names = sorted({str(c) for c in colours if isinstance(c, str) and c != "."})
                if names:
                    note = "; ".join(filter(None, [note, "set from: " + ", ".join(names)]))
                    colours = ["?" if c in names else c for c in colours]
                rows.append("\t".join([
                    fname, name, str(first + n + 1), style, outline, " ".join(map(str, colours)),
                    " ".join(hexes(colours, pal)),
                    "yes" if depth > 1 else "", note]))
        else:
            pending.append(raw)
            om = OUTLINE.search(line)
            if om:
                outline = str(number(om.group(1)))
        depth += line.count("{") - line.count("}")
    return rows


if __name__ == "__main__":
    sys.exit(main())
