#!/usr/bin/env bash
# Save pages of the Master of Magic Wiki into reference/ through Safari (macOS, AppleScript).
#
# Why Safari: Fandom answers curl and the fetch tools with HTTP 402; a real browser is let in.
# The script opens each page in a new Safari window, waits for it to load, saves the page source
# as `reference/<Title> _ Master of Magic Wiki _ Fandom.html` (the name a browser's "Save as"
# gives, which `scripts/strip_wiki.py` expects) and closes the window again.
#
# Usage: scripts/fetch_wiki.sh [--no-strip] [--text] PAGE...
set -euo pipefail

WIKI_PAGE_URL="https://masterofmagic.fandom.com/wiki/"
WIKI_FILE_SUFFIX=" _ Master of Magic Wiki _ Fandom.html"
LOAD_TIMEOUT_S=30
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REFERENCE="$ROOT/reference"

usage() {
  cat <<'EOF'
fetch_wiki.sh: save Master of Magic Wiki pages into reference/ through Safari.

Usage: scripts/fetch_wiki.sh [--no-strip] [--text] PAGE...

PAGE is a page title as in the wiki's address, with spaces or underscores: "Melee Attack",
Melee_Attack, "Haste". Each page is saved as reference/<Title> _ Master of Magic Wiki _ Fandom.html
and then stripped to the readable article with scripts/strip_wiki.py.

Options:
  --no-strip   keep the page source as Safari gives it (about 0.5 MB a page)
  --text       also print the page's plain text (what a reader sees), to read it at once
  -h, --help   this text; it runs nothing

Needs: macOS, Safari, and the first time a permission for the terminal to control Safari (System
Settings > Privacy & Security > Automation). Nothing else: no Develop-menu setting is used.
It touches only the window it opens. Exit 0 when every page was saved, 1 otherwise.
Example: scripts/fetch_wiki.sh --text "Melee Attack" Haste
EOF
}

safari_fetch() { # title -> page source on stdout; text on fd 3 when wanted
  local url="$1" want_text="$2" text_file="$3"
  osascript - "$url" "$LOAD_TIMEOUT_S" "$want_text" "$text_file" <<'APPLESCRIPT'
on run argv
  set pageUrl to item 1 of argv
  set timeoutS to (item 2 of argv) as integer
  set wantText to item 3 of argv
  set textFile to item 4 of argv
  tell application "Safari"
    make new document
    set URL of front document to pageUrl
    set waited to 0
    repeat while waited < timeoutS
      delay 1
      set waited to waited + 1
      set t to name of front document
      if t is not "Start Page" and t is not "" then exit repeat
    end repeat
    delay 2
    set src to source of front document
    if wantText is "yes" then
      set body to text of front document
      set fh to open for access POSIX file textFile with write permission
      set eof fh to 0
      write body to fh as «class utf8»
      close access fh
    end if
    close front document
    return src
  end tell
end run
APPLESCRIPT
}

main() {
  local strip=yes want_text=no pages=()
  while [ $# -gt 0 ]; do
    case "$1" in
      -h|--help) usage; exit 0 ;;
      --no-strip) strip=no ;;
      --text) want_text=yes ;;
      -*) echo "fetch_wiki.sh: unknown option: $1" >&2; exit 1 ;;
      *) pages+=("$1") ;;
    esac
    shift
  done
  if [ ${#pages[@]} -eq 0 ]; then
    echo "fetch_wiki.sh: no page given (see --help)" >&2
    exit 1
  fi
  mkdir -p "$REFERENCE"
  local failed=0 title slug file text_file
  for title in "${pages[@]}"; do
    title="${title//_/ }"
    slug="${title// /_}"
    file="$REFERENCE/$title$WIKI_FILE_SUFFIX"
    text_file="$(mktemp)"
    if safari_fetch "$WIKI_PAGE_URL$slug" "$want_text" "$text_file" > "$file.part" \
        && grep -q "<html" "$file.part"; then
      mv "$file.part" "$file"
      echo "saved: $file"
      [ "$strip" = yes ] && python3 "$ROOT/scripts/strip_wiki.py" "$file"
      [ "$want_text" = yes ] && cat "$text_file"
    else
      rm -f "$file.part"
      echo "fetch_wiki.sh: could not save the page: $title" >&2
      failed=1
    fi
    rm -f "$text_file"
  done
  exit "$failed"
}

main "$@"
