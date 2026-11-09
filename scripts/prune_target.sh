#!/bin/bash
# Keep target/debug from growing without bound. Cargo never removes the old test binaries and
# incremental caches of earlier source states, and it once reached about 600 GB. Everything in
# target/debug is rebuilt from source (a cold build is about a minute), so it is safe to drop.
#
#   scripts/prune_target.sh [--max-gb N] [--dry-run]   clean target/debug when it is over N GB
#   scripts/prune_target.sh --install-hook             install .git/hooks/pre-push to run it
#   scripts/prune_target.sh -h | --help                print the usage
#
# The default limit is 20 GB: under it nothing is deleted, so a push does not cost a rebuild; use
# `--max-gb 0` to clean on every push. The pre-push hook runs this in the background and never
# blocks or fails the push (deleting a very large target/ takes a long time).
set -u

max_gb=20
dry_run=0
repo=$(cd "$(dirname "$0")/.." && pwd)

usage() { grep '^#   scripts' "$0" | sed 's/^#   //'; }

install_hook() {
    local hook="$repo/.git/hooks/pre-push"
    if [ -e "$hook" ]; then
        echo "error: $hook already exists; add a call to scripts/prune_target.sh to it by hand" >&2
        exit 1
    fi
    printf '#!/bin/sh\n# Drop an oversized target/debug (scripts/prune_target.sh); never blocks the push.\n"$(git rev-parse --show-toplevel)/scripts/prune_target.sh" >/dev/null 2>&1 &\nexit 0\n' >"$hook"
    chmod +x "$hook"
    echo "installed $hook"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --max-gb) max_gb="${2:?--max-gb needs a number}"; shift 2 ;;
        --dry-run) dry_run=1; shift ;;
        --install-hook) install_hook; exit 0 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "error: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

# target/debug (dev and test profiles) and target/debug-full (the full-debuginfo profile).
dirs=()
for name in debug debug-full; do
    [ -d "$repo/target/$name" ] && dirs+=("$repo/target/$name")
done
[ ${#dirs[@]} -gt 0 ] || { echo "nothing to prune: no target/debug"; exit 0; }

size_gb=$(( $(du -sk "${dirs[@]}" | awk '{s += $1} END {print s}') / 1024 / 1024 ))
if [ "$size_gb" -le "$max_gb" ]; then
    echo "target/debug* is $size_gb GB, within the $max_gb GB limit"
    exit 0
fi

echo "target/debug* is $size_gb GB, over the $max_gb GB limit"
if [ "$dry_run" -eq 1 ]; then
    echo "dry run: would run cargo clean for the dev and debug-full profiles"
    exit 0
fi
cd "$repo" || exit 1
cargo clean --profile dev
[ -d target/debug-full ] && cargo clean --profile debug-full
exit 0
