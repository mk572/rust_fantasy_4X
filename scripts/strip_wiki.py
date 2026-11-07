#!/usr/bin/env python3
"""Strip the saved Master of Magic wiki pages in reference/ down to what is needed to read them.

A page saved from Fandom carries scripts, trackers, ads, video players, the site's global
header, the right rail and footer. This keeps the page title, the wiki's own menu (links only)
and the article with its images, in one small stylesheet of its own, and removes the files in
the page's `_files` folder that the stripped page no longer uses (scripts, styles, frames,
tracker pixels, videos, ad images).

    python3 scripts/strip_wiki.py [--dry-run] [PAGE.html ...]

With no page, every `reference/* Master of Magic Wiki _ Fandom.html` is stripped. The stripped
page is written over the original, so keep a backup; a second run changes nothing.
"""
import argparse
import html
import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote

REFERENCE = Path(__file__).resolve().parent.parent / "reference"
VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr"}
# Elements that never belong in a readable page.
DROP_TAGS = {"script", "style", "noscript", "iframe", "video", "audio", "source", "track",
             "svg", "button", "form", "input", "select", "textarea", "link", "meta", "canvas",
             "object", "embed", "template"}
# Class or id fragments of ads, video players, editing tools and page chrome.
DROP_WORDS = ("cnx", "gvtf", "floating-close", "ad-slot", "top-ads", "bottom-ads", "wikia-ad",
              "adincontent", "ad-unit", "mw-editsection", "page-side-tool", "page-header__actions",
              "page-header__categories", "page-header__meta", "notifications", "uncrawlable",
              "article-categories", "global-", "toc-edit", "mw-empty-elt")
KEEP_ATTRS = {"href", "src", "alt", "width", "height", "colspan", "rowspan", "id", "class",
              "title", "lang"}


class Node:
    def __init__(self, tag, attrs, parent=None):
        self.tag, self.attrs, self.parent = tag, dict(attrs), parent
        self.children = []  # Node or str


class Builder(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = Node("#root", [])
        self.cur = self.root

    def handle_starttag(self, tag, attrs):
        node = Node(tag, attrs, self.cur)
        self.cur.children.append(node)
        if tag not in VOID:
            self.cur = node

    def handle_startendtag(self, tag, attrs):
        self.cur.children.append(Node(tag, attrs, self.cur))

    def handle_endtag(self, tag):
        n = self.cur
        while n is not self.root and n.tag != tag:
            n = n.parent
        if n is not self.root:
            self.cur = n.parent

    def handle_data(self, data):
        self.cur.children.append(data)


def parse(text):
    b = Builder()
    b.feed(text)
    return b.root


def walk(node):
    for c in node.children:
        if isinstance(c, Node):
            yield c
            yield from walk(c)


def classes(node):
    return (node.attrs.get("class") or "") + " " + (node.attrs.get("id") or "")


def find(node, tag, word):
    for n in walk(node):
        if n.tag == tag and word in (n.attrs.get("class") or "").split():
            return n
    return None


def junk(node):
    if node.tag in DROP_TAGS:
        return True
    c = classes(node).lower()
    return any(w in c for w in DROP_WORDS)


def image_src(node):
    src = node.attrs.get("src") or ""
    if src.startswith("data:") or not src:
        src = node.attrs.get("data-src") or src
    return src


def render(node, out, used):
    if isinstance(node, str):
        out.append(html.escape(node, quote=False))
        return
    if junk(node):
        return
    attrs = {k: v for k, v in node.attrs.items() if k in KEEP_ATTRS and v is not None}
    if node.tag == "img":
        src = image_src(node)
        if not src or src.startswith("data:"):
            return
        attrs["src"] = src
        used.add(unquote(src))
    if node.tag == "a":
        href = attrs.get("href", "")
        # Links to saved files (images opened full size) keep their target.
        if href and not re.match(r"[a-z]+:|#|/", href):
            used.add(unquote(href))
    if node.tag == "a" and not attrs.get("href"):
        for c in node.children:
            render(c, out, used)
        return
    text = "".join(
        f' {k}="{html.escape(v, quote=True)}"' for k, v in attrs.items() if k not in ("class", "id")
    ) if node.tag not in ("figure",) else "".join(
        f' {k}="{html.escape(v, quote=True)}"' for k, v in attrs.items() if k not in ("id",)
    )
    out.append(f"<{node.tag}{text}>")
    if node.tag in VOID:
        return
    for c in node.children:
        render(c, out, used)
    out.append(f"</{node.tag}>")


MARK = "<!-- stripped by scripts/strip_wiki.py -->"
STYLE = """body{font:16px/1.5 system-ui,sans-serif;max-width:60rem;margin:1rem auto;padding:0 1rem;color:#222}
nav{background:#eee;padding:.4rem .8rem;border-radius:4px;margin-bottom:1rem}
nav ul{list-style:none;margin:0;padding:0}nav li{display:inline-block;margin-right:1rem}
nav ul ul{display:block}nav ul ul li{display:inline-block}
table{border-collapse:collapse;margin:1rem 0}td,th{border:1px solid #bbb;padding:.2rem .5rem}
img{max-width:100%;height:auto}figure{margin:1rem}.thumbcaption{font-size:.85em;color:#555}
a{color:#06c}"""


def title_of(root):
    for n in walk(root):
        if n.tag == "title":
            return "".join(c for c in n.children if isinstance(c, str)).strip()
    return ""


def strip_page(text):
    """Return (stripped html, the set of local paths it uses)."""
    root = parse(text)
    used = set()
    title = find(root, "h1", "page-header__title")
    menu = find(root, "nav", "fandom-community-header__local-navigation")
    body = find(root, "div", "mw-body-content")
    if body is None:
        body = find(root, "div", "mw-parser-output")
    out = [MARK + "\n<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">",
           f"<title>{html.escape(title_of(root))}</title><style>{STYLE}</style></head><body>\n"]
    if menu is not None:
        render(menu, out, used)
        out.append("\n")
    if title is not None:
        out.append("<h1>")
        out.extend(html.escape("".join(
            c if isinstance(c, str) else "".join(x for x in c.children if isinstance(x, str))
            for c in title.children).strip()))
        out.append("</h1>\n")
    if body is not None:
        render(body, out, used)
    out.append("\n</body></html>\n")
    return "".join(out), used


def folder_of(page):
    return page.with_name(page.name[: -len(".html")] + "_files")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("pages", nargs="*")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    pages = [Path(p) for p in args.pages] or sorted(REFERENCE.glob("* Master of Magic Wiki _ Fandom.html"))
    for page in pages:
        text = page.read_text(encoding="utf-8", errors="replace")
        if text.startswith(MARK):
            print(f"{page.name}: already stripped")
            continue
        new, used = strip_page(text)
        if "mw-body-content" not in text and "mw-parser-output" not in text:
            print(f"{page.name}: no article found, left alone")
            continue
        folder = folder_of(page)
        keep = {Path(u).name for u in used if folder.name in u}
        drop = [f for f in folder.iterdir() if f.is_file() and f.name not in keep] if folder.is_dir() else []
        freed = sum(f.stat().st_size for f in drop)
        print(f"{page.name}: {len(text) // 1024} KB -> {len(new) // 1024} KB; "
              f"{len(keep)} files kept, {len(drop)} dropped ({freed // 1024} KB)")
        if args.dry_run:
            continue
        page.write_text(new, encoding="utf-8")
        for f in drop:
            f.unlink()
    return 0


if __name__ == "__main__":
    sys.exit(main())
