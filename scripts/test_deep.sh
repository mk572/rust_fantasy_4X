#!/bin/bash
# The deep test run: the tests that sweep a range, every world, every damage, with every value
# (F4X_DEEP=1; the normal run tries the ends and the default), optionally the mutation tests.
#
#   scripts/test_deep.sh                 every test of the workspace, deep (about four minutes)
#   scripts/test_deep.sh FILTER...       only the tests whose name matches, for finding where one breaks
#   scripts/test_deep.sh --mutants CRATE FILE...   cargo-mutants on the given source files of CRATE
#                                        (hours; runs under scripts/cool_run.sh; output in /tmp/mutants-CRATE)
#   scripts/test_deep.sh -h | --help     print this
set -eu
cd "$(dirname "$0")/.."
usage() { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; }
case "${1:-}" in
-h | --help)
    usage
    exit 0
    ;;
--mutants)
    shift
    [ $# -ge 2 ] || { usage >&2; exit 2; }
    crate=$1
    shift
    files=()
    for f in "$@"; do files+=(--file "$f"); done
    exec scripts/cool_run.sh cargo mutants -p "$crate" --output "/tmp/mutants-$crate" "${files[@]}" \
        -- --features testutil
    ;;
esac
export F4X_DEEP=1
exec cargo nextest run --workspace --exclude f4x-learn --no-fail-fast "$@"
