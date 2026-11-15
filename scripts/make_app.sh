#!/bin/sh
# Build the macOS apps for Apple silicon, with none of the original's files (docs/installer-design.md
# sections 6.5 and 10): target/Fantasy4X.app (the game) and "target/Fantasy 4X Setup.app" (the setup
# program that extracts the pictures from the player's own copy). Each holds the release program, the
# shipped rules, texts and client tables, an icon and an Info.plist, signed ad hoc (no developer
# account; the right-click Open steps are in packaging/macos/README-macOS.txt).
#
#   scripts/make_app.sh [--dist] [--link-assets] [--open]
#   scripts/make_app.sh -h | --help
set -eu
usage() {
    cat <<'USAGE'
Usage: make_app.sh [--dist] [--link-assets] [--open]
       make_app.sh -h | --help

Build target/Fantasy4X.app (the game) and "target/Fantasy 4X Setup.app" (the setup program) for Apple
silicon: the release builds of f4x-client and f4x-setup, the shipped files (crates/f4x-def/content,
crates/f4x-lang/content, crates/f4x-client/content), the icon and an Info.plist, signed ad hoc. They
carry nothing of the original game. The game finds the player's pictures and sounds, and writes its
saves, in ~/Library/Application Support/Fantasy4X/; the setup writes the pictures there.

Options:
  --dist          also make target/Fantasy4X-<version>-macos-arm64.dmg and .zip
                  (both apps, README-macOS.txt, LICENSE and a link to Applications)
  --link-assets   for play on this machine: point ~/Library/Application Support/Fantasy4X/assets-local
                  at this repository's assets-local (the original's art and sound, extracted by
                  f4x-extract); this is outside the project, so it only happens when asked for
  --open          open the game when it is built
  -h, --help      print this help and exit

Needs: cargo and an Apple silicon Mac (a release build, codesign, hdiutil, ditto). Runs from anywhere
(it changes to the repo root). Writes: the two apps in target/, replacing earlier ones, and with
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

# Two builds, not one: the setup does not need the game's sound, and one build would join the features.
cargo build --release -p f4x-client
cargo build --release -p f4x-setup

# make_bundle APP_DIR PROGRAM DISPLAY_NAME IDENTIFIER: one app with the shipped files, icon and plist.
make_bundle() {
    bundle=$1
    program=$2
    res="$bundle/Contents/Resources"
    mkdir -p "$bundle/Contents/MacOS" "$res/crates/f4x-def" "$res/crates/f4x-lang" "$res/crates/f4x-client"
    cp "target/release/$program" "$bundle/Contents/MacOS/$program"
    cp -R crates/f4x-def/content "$res/crates/f4x-def/content"
    cp -R crates/f4x-lang/content "$res/crates/f4x-lang/content"
    cp -R crates/f4x-client/content "$res/crates/f4x-client/content"
    cp packaging/macos/Fantasy4X.icns "$res/Fantasy4X.icns"
    find "$bundle" \( -name '.DS_Store' -o -name '*.orig' \) -type f -delete
    cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$3</string>
<key>CFBundleDisplayName</key><string>$3</string>
<key>CFBundleIdentifier</key><string>$4</string>
<key>CFBundleExecutable</key><string>$program</string>
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
    codesign --force --deep --sign - "$bundle"
    codesign --verify --deep --strict "$bundle"
}

app=target/Fantasy4X.app
setup_app="target/Fantasy 4X Setup.app"
rm -rf target/Fantasy4X.app
rm -rf "target/Fantasy 4X Setup.app"
make_bundle "$app" f4x-client "Fantasy 4X" io.github.fantasy4x
make_bundle "$setup_app" f4x-setup "Fantasy 4X Setup" io.github.fantasy4x.setup

if [ "$dist" = 1 ]; then
    name="Fantasy4X-$version-macos-arm64"
    stage=target/dmg-stage
    rm -rf target/dmg-stage
    mkdir -p "$stage"
    ditto "$app" "$stage/Fantasy4X.app"
    ditto "$setup_app" "$stage/Fantasy 4X Setup.app"
    cp packaging/macos/README-macOS.txt LICENSE "$stage/"
    ln -s /Applications "$stage/Applications"
    rm -f "target/$name.dmg" "target/$name.zip"
    hdiutil create -quiet -volname "Fantasy 4X" -srcfolder "$stage" -format UDZO "target/$name.dmg"
    (cd "$stage" && zip -q -r -y "../$name.zip" Fantasy4X.app "Fantasy 4X Setup.app" README-macOS.txt LICENSE)
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

echo "built $app and $setup_app ($version)"
[ "$open_after" = 1 ] && open "$app"
exit 0
