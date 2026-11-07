#!/usr/bin/env python3
"""Freeze the scripted strategic AI as the reference opponent of `f4x-tools soak-versus`.

Copies `crates/f4x-ai` to `crates/f4x-ai-ref` (replacing the previous reference), renames the
package, and turns off its own tests (the live crate's tests already cover the same code). Edit
`f4x-ai` afterwards; the reference keeps the old behaviour, and plays the neutral cities and
monsters in every versus game.

    python3 scripts/fork_ai.py [--from REF]
    python3 scripts/fork_ai.py --mark-stale [REASON]
    python3 scripts/fork_ai.py --is-stale

`--from REF` freezes the AI as it was at a git ref (a commit or tag) instead of the working tree.

The reference is outside the workspace (`exclude` in the root `Cargo.toml`), so a change to the
core never breaks `cargo build/test --workspace`. When a change does break it, do not repair it:
`--mark-stale` writes `crates/f4x-ai-ref/STALE`, its `build.rs` then refuses to build it, and
`scripts/ai_battle.py` forks it again from the live AI before the next battle. A fork removes
the marker. The copy is a standalone package (no workspace inheritance) with that `build.rs`.
"""
import argparse
import io
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIVE = ROOT / "crates" / "f4x-ai"
REFERENCE = ROOT / "crates" / "f4x-ai-ref"
MARKER = REFERENCE / "STALE"

BUILD_SCRIPT = """\
// Written by scripts/fork_ai.py. A stale reference is not built: fork it again.
fn main() {
    println!("cargo:rerun-if-changed=STALE");
    if std::path::Path::new("STALE").exists() {
        panic!(
            "f4x-ai-ref is stale (crates/f4x-ai-ref/STALE): run `python3 scripts/ai_battle.py ...` \
             or `python3 scripts/fork_ai.py`"
        );
    }
}
"""


def copy_tree(ref: str | None) -> None:
    """Put a copy of the live crate's manifest and sources in the reference folder."""
    if REFERENCE.exists():
        shutil.rmtree(REFERENCE)
    REFERENCE.mkdir(parents=True)
    if ref is None:
        shutil.copy(LIVE / "Cargo.toml", REFERENCE / "Cargo.toml")
        shutil.copytree(LIVE / "src", REFERENCE / "src", copy_function=shutil.copy)  # new mtimes, or cargo keeps a stale build
        return
    archive = subprocess.run(
        ["git", "archive", ref, "crates/f4x-ai/Cargo.toml", "crates/f4x-ai/src"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            member.name = member.name.removeprefix("crates/f4x-ai/")
            if member.name:
                tar.extract(member, REFERENCE, filter="data")


def workspace_settings() -> tuple[dict, dict]:
    """The root manifest's `workspace.package` and `workspace.dependencies`."""
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    return workspace["package"], workspace["dependencies"]


def toml_value(value) -> str:
    if isinstance(value, dict):
        inner = ", ".join(f"{k} = {toml_value(v)}" for k, v in value.items())
        return "{ " + inner + " }"
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    if isinstance(value, bool):
        return "true" if value else "false"
    return f'"{value}"'


def dependency_line(name: str, spec, shared: dict) -> str:
    """A dependency without workspace inheritance: the root's spec, plus the crate's own features."""
    base = shared[name]
    base = {"version": base} if isinstance(base, str) else dict(base)
    if "path" in base:
        base["path"] = "../" + Path(base["path"]).name
    extra = spec if isinstance(spec, dict) else {}
    if "features" in extra:
        base["features"] = sorted(set(base.get("features", [])) | set(extra["features"]))
    if extra.get("optional"):
        base["optional"] = True
    if set(base) == {"version"}:
        return f'{name} = "{base["version"]}"'
    return f"{name} = {toml_value(base)}"


def rewrite_manifest() -> None:
    """The copy is its own package, outside the workspace: no `.workspace = true` anywhere."""
    package, shared = workspace_settings()
    manifest = REFERENCE / "Cargo.toml"
    parsed = tomllib.loads(manifest.read_text())
    lines = [
        "[package]",
        'name = "f4x-ai-ref"',
        'edition = "%s"' % package["edition"],
        'version = "%s"' % package["version"],
        'license = "%s"' % package["license"],
        'build = "build.rs"',
        "",
        "[lib]",
        "test = false",
        "doctest = false",
    ]
    for table in ("dependencies", "dev-dependencies"):
        if table in parsed:
            lines += ["", f"[{table}]"]
            for name, spec in parsed[table].items():
                inherited = isinstance(spec, dict) and spec.get("workspace") or spec == {"workspace": True}
                lines.append(dependency_line(name, spec, shared) if inherited else f"{name} = {toml_value(spec)}")
    manifest.write_text("\n".join(lines) + "\n")
    (REFERENCE / "build.rs").write_text(BUILD_SCRIPT)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--from", dest="ref", help="a git ref to freeze instead of the working tree")
    parser.add_argument("--mark-stale", nargs="?", const="broken by a change", metavar="REASON",
                        help="write the STALE marker instead of forking")
    parser.add_argument("--is-stale", action="store_true", help="exit 1 if the marker is there")
    args = parser.parse_args()
    if args.is_stale:
        return 1 if MARKER.exists() else 0
    if args.mark_stale:
        MARKER.write_text(args.mark_stale + "\n")
        print(f"{MARKER.relative_to(ROOT)} written: the next battle forks the reference again")
        return 0
    copy_tree(args.ref)
    rewrite_manifest()
    source = f"git ref {args.ref}" if args.ref else "the working tree"
    print(f"crates/f4x-ai-ref is now a copy of f4x-ai from {source}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
