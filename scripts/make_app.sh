#!/bin/sh
# Build a local macOS app: target/Fantasy4X.app (release build of f4x-client plus a copy of
# content/, content-local/ and assets-local/art and screens). For play on this machine only: it carries the
# local-only original art and text, so do not share it. Saves go to
# ~/Library/Application Support/Fantasy4X/saves.
#
#   scripts/make_app.sh [--open]
#   scripts/make_app.sh -h | --help
set -eu
usage() {
    cat <<'USAGE'
Usage: make_app.sh [--open]
       make_app.sh -h | --help

Build a local macOS app, target/Fantasy4X.app: a release build of f4x-client plus a copy of
content/, content-local/ and assets-local/art, screens and audio. For play on this machine only: it carries the
local-only original art and text, so do not share it. Saves go to
~/Library/Application Support/Fantasy4X/saves.

Options:
  --open        open the app when it is built
  -h, --help    print this help and exit

Needs: cargo (a release build of f4x-client), run from anywhere (it changes to the repo root).
Writes: target/Fantasy4X.app, replacing a previous one. Exit status 2 for a usage error.
USAGE
}
open_after=0
for arg in "$@"; do
    case "$arg" in
        -h|--help) usage; exit 0 ;;
        --open) open_after=1 ;;
        *) echo "make_app: unknown option: $arg (see --help)" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

cargo build --release -p f4x-client

app=target/Fantasy4X.app
rm -rf target/Fantasy4X.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/assets-local"
cp target/release/f4x-client "$app/Contents/MacOS/f4x-client"
cp -R content "$app/Contents/Resources/content"
[ -d content-local ] && cp -R content-local "$app/Contents/Resources/content-local"
[ -d assets-local/art ] && cp -R assets-local/art "$app/Contents/Resources/assets-local/art"
[ -f assets-local/index.tsv ] && cp assets-local/index.tsv "$app/Contents/Resources/assets-local/index.tsv"
[ -d assets-local/screens ] && cp -R assets-local/screens "$app/Contents/Resources/assets-local/screens"
[ -d assets-local/audio ] && cp -R assets-local/audio "$app/Contents/Resources/assets-local/audio"

# The game finds content/, content-local/, assets-local/ and saves/ relative to the working
# directory, so the launcher runs it from a folder that links to the bundled copies.
cat > "$app/Contents/MacOS/Fantasy4X" <<'LAUNCH'
#!/bin/sh
here="$(cd "$(dirname "$0")" && pwd)"
res="$here/../Resources"
data="$HOME/Library/Application Support/Fantasy4X"
mkdir -p "$data/saves"
for d in content content-local assets-local; do
    rm -f "$data/$d"
    [ -e "$res/$d" ] && ln -s "$res/$d" "$data/$d"
done
cd "$data"
exec "$here/f4x-client" "$@"
LAUNCH
chmod +x "$app/Contents/MacOS/Fantasy4X"

cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fantasy 4X</string>
<key>CFBundleDisplayName</key><string>Fantasy 4X</string>
<key>CFBundleIdentifier</key><string>local.fantasy4x</string>
<key>CFBundleExecutable</key><string>Fantasy4X</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

echo "built $app"
[ "$open_after" = 1 ] && open "$app"
exit 0
