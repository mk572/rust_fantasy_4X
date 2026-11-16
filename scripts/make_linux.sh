#!/bin/sh
# Build the Linux package, with none of the original's files (docs/installer-design.md 6.5, 10):
# target/Fantasy4X-<version>-linux-<arch>/ and a .tar.gz of it. It holds the game (f4x-client), the setup
# (f4x-setup), the music program (f4x-music), the shipped rules, texts and client tables, the licences and
# a README. Run it on the Linux machine it is for (x86_64 or aarch64); the release workflow does the same.
#
#   scripts/make_linux.sh
#   scripts/make_linux.sh -h | --help
set -eu
usage() {
    cat <<'USAGE'
Usage: make_linux.sh
       make_linux.sh -h | --help

Build target/Fantasy4X-<version>-linux-<arch>/ and target/Fantasy4X-<version>-linux-<arch>.tar.gz from the
release builds of f4x-client, f4x-setup and tools/xmi-play's f4x-music. Needs a Linux machine with cargo, a C
compiler, pkg-config and the development packages of the game's libraries (Debian/Ubuntu: build-essential
pkg-config libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev; the git submodule third_party/mt32-rs must
be checked out). Runs from anywhere (it changes to the repo root). Exit status 2 for a usage error.
USAGE
}
for arg in "$@"; do
    case "$arg" in
        -h|--help) usage; exit 0 ;;
        *) echo "make_linux: unknown option: $arg (see --help)" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

if [ "$(uname -s)" != "Linux" ]; then
    echo "make_linux: this builds the Linux package and needs a Linux machine" >&2
    exit 1
fi
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$version" ] || { echo "make_linux: no workspace version in Cargo.toml" >&2; exit 1; }
arch=$(uname -m)
[ -f third_party/mt32-rs/Cargo.toml ] || { echo "make_linux: run git submodule update --init first" >&2; exit 1; }

# Separate builds: the setup does not need the game's sound, and one build would join the features.
cargo build --release -p f4x-client
cargo build --release -p f4x-setup
cargo build --release --manifest-path tools/xmi-play/Cargo.toml --bin f4x-music

name="Fantasy4X-$version-linux-$arch"
out="target/$name"
rm -rf "target/$name"
rm -f "target/$name.tar.gz"
mkdir -p "$out/crates/f4x-def" "$out/crates/f4x-lang" "$out/crates/f4x-client" "$out/legal"
cp target/release/f4x-client target/release/f4x-setup tools/xmi-play/target/release/f4x-music "$out/"
cp -R crates/f4x-def/content "$out/crates/f4x-def/content"
cp -R crates/f4x-lang/content "$out/crates/f4x-lang/content"
cp -R crates/f4x-client/content "$out/crates/f4x-client/content"
cp third_party/mt32-rs/LGPL21.txt third_party/mt32-rs/AUTHORS.txt packaging/macos/MUSIC-LICENSE.txt "$out/legal/"
cp packaging/linux/README-linux.txt packaging/linux/fantasy4x.png LICENSE "$out/"
find "$out" \( -name '.DS_Store' -o -name '*.orig' \) -type f -delete
tar -C target -czf "target/$name.tar.gz" "$name"
echo "made target/$name.tar.gz"
