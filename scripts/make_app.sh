#!/bin/sh
# Build the macOS app for Apple silicon: target/Fantasy4X.app, with none of the original's files
# (docs/installer-design.md sections 6.5 and 10). The release client, the shipped rules, texts and
# client tables, an icon and an Info.plist, signed ad hoc (no developer account; the right-click
# Open steps are in packaging/macos/README-macOS.txt).
#
#   scripts/make_app.sh [--dist] [--link-assets] [--open]
#   scripts/make_app.sh -h | --help
set -eu
usage() {
    cat <<'USAGE'
Usage: make_app.sh [--dist] [--link-assets] [--open]
       make_app.sh -h | --help

Build target/Fantasy4X.app (Apple silicon): the release build of f4x-client, the shipped files
(crates/f4x-def/content, crates/f4x-lang/content, crates/f4x-client/content), the icon and an
Info.plist, signed ad hoc. It carries nothing of the original game. The game finds the player's
pictures and sounds, and writes its saves, in ~/Library/Application Support/Fantasy4X/.

Options:
  --dist          also make target/Fantasy4X-<version>-macos-arm64.dmg and .zip
                  (the app, README-macOS.txt, LICENSE and a link to Applications)
  --link-assets   for play on this machine: point ~/Library/Application Support/Fantasy4X/assets-local
                  at this repository's assets-local (the original's art and sound, extracted by
                  f4x-extract); this is outside the project, so it only happens when asked for
  --open          open the app when it is built
  -h, --help      print this help and exit

Needs: cargo and an Apple silicon Mac (a release build, codesign, hdiutil, ditto). Runs from anywhere
(it changes to the repo root). Writes: target/Fantasy4X.app, replacing a previous one, and with
--dist the dmg and zip. Exit status 2 for a usage error.
USAGE
}
dist=0
link_assets=0
open_after=0
for arg in "$@"; do
    case "$arg" in
        -h|--help) usage; exit 0 ;;
        --dist) dist=1 ;;
        --link-assets) link_assets=1 ;;
        --open) open_after=1 ;;
        *) echo "make_app: unknown option: $arg (see --help)" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

if [ "$(uname -m)" != "arm64" ]; then
    echo "make_app: this builds the Apple silicon app and needs an Apple silicon Mac" >&2
    exit 1
fi
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$version" ] || { echo "make_app: no workspace version in Cargo.toml" >&2; exit 1; }

cargo build --release -p f4x-client

app=target/Fantasy4X.app
res="$app/Contents/Resources"
rm -rf target/Fantasy4X.app
mkdir -p "$app/Contents/MacOS" "$res/crates/f4x-def" "$res/crates/f4x-lang" "$res/crates/f4x-client"
cp target/release/f4x-client "$app/Contents/MacOS/f4x-client"
cp -R crates/f4x-def/content "$res/crates/f4x-def/content"
cp -R crates/f4x-lang/content "$res/crates/f4x-lang/content"
cp -R crates/f4x-client/content "$res/crates/f4x-client/content"
cp packaging/macos/Fantasy4X.icns "$res/Fantasy4X.icns"
find "$app" \( -name '.DS_Store' -o -name '*.orig' \) -type f -delete

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fantasy 4X</string>
<key>CFBundleDisplayName</key><string>Fantasy 4X</string>
<key>CFBundleIdentifier</key><string>io.github.fantasy4x</string>
<key>CFBundleExecutable</key><string>f4x-client</string>
<key>CFBundleIconFile</key><string>Fantasy4X</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>$version</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>LSApplicationCategoryType</key><string>public.app-category.strategy-games</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

# Ad hoc signature: seals the bundle so Apple silicon starts it; it is not a developer signature.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

if [ "$dist" = 1 ]; then
    name="Fantasy4X-$version-macos-arm64"
    stage=target/dmg-stage
    rm -rf target/dmg-stage
    mkdir -p "$stage"
    ditto "$app" "$stage/Fantasy4X.app"
    cp packaging/macos/README-macOS.txt LICENSE "$stage/"
    ln -s /Applications "$stage/Applications"
    rm -f "target/$name.dmg" "target/$name.zip"
    hdiutil create -quiet -volname "Fantasy 4X" -srcfolder "$stage" -format UDZO "target/$name.dmg"
    ditto -c -k --keepParent "$app" "target/$name.zip"
    rm -rf target/dmg-stage
    echo "made target/$name.dmg and target/$name.zip"
fi

if [ "$link_assets" = 1 ]; then
    data="$HOME/Library/Application Support/Fantasy4X"
    mkdir -p "$data"
    rm -f "$data/assets-local"
    ln -s "$PWD/assets-local" "$data/assets-local"
    echo "linked $data/assets-local -> $PWD/assets-local"
fi

echo "built $app ($version)"
[ "$open_after" = 1 ] && open "$app"
exit 0
