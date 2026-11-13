#!/bin/bash
# Variety soaks: AI wizards play whole games on different map sizes, land shares, node strengths,
# difficulties and numbers of wizards, to uncover problems the standard soak seeds do not (a game
# nobody wins, a wizard cut off on an island, a refused-order storm, a panic). Runs politely
# (scripts/cool_run.sh) into one folder.
#
#   scripts/soak_variety.sh [options]        see --help
set -u

usage() {
    cat <<'USAGE'
Usage: soak_variety.sh [--out DIR] [--only NAME[,NAME...]] [--jobs N] [--quick] [--dry-run]
                       [--seed-offset N] [--extra 'SOAK ARGS']
       soak_variety.sh -h | --help

Play a set of `f4x-tools soak` runs that differ in map size, land share, node strength, difficulty
and the number of wizards, each on its own seeds, with nobody watching.

Options:
  --out DIR     folder for the reports (default /tmp/soak-variety-YYYYmmdd-HHMM); one <name>.txt
                and one <name>-refused.tsv for each run, and summary.txt
  --only LIST   run only these runs (comma-separated names, see the list below)
  --jobs N      games played at once in a run (default 4)
  --quick       one seed and 300 turns for every run: a smoke test of the matrix
  --dry-run     print the commands and stop
  --seed-offset N  add N to every seed of the runs: more games for the same settings (default 0)
  --extra ARGS  more arguments for every `f4x-tools soak` (one quoted string, e.g. '--casts --events')

Runs (name: wizards, map, land %, node strength, difficulty, seeds, turns):
  small-2p     2  45x30  20  1  2  101..104  500
  crowded-5p   5  60x40  35  1  2  111..113  600
  huge-4p      4  90x60  30  1  2  121..123  700
  islands-3p   3  60x40  15  1  2  131..134  600
  magic-4p     4  60x40  30  2  2  141..143  600
  weak-3p      3  60x40  30  0  2  151..153  600
  hard-3p      3  60x40  30  1  4  161..163  600
  easy-2p      2  60x40  30  1  1  171..173  500
  long-2p      2  60x40  30  1  2  181..182  1000

The summary lists, for each run, the games that ended with a winner, the games that panicked, the
defeated wizards and the refused orders. Needs: cargo, scripts/cool_run.sh.
Exit status: 0 when the runs finished, 1 when a run failed or panicked, 2 for a usage error.
USAGE
}

RUNS=(
    "small-2p|2|45x30|20|1|2|101..104|500"
    "crowded-5p|5|60x40|35|1|2|111..113|600"
    "huge-4p|4|90x60|30|1|2|121..123|700"
    "islands-3p|3|60x40|15|1|2|131..134|600"
    "magic-4p|4|60x40|30|2|2|141..143|600"
    "weak-3p|3|60x40|30|0|2|151..153|600"
    "hard-3p|3|60x40|30|1|4|161..163|600"
    "easy-2p|2|60x40|30|1|1|171..173|500"
    "long-2p|2|60x40|30|1|2|181..182|1000"
)

out=""; only=""; jobs=4; quick=0; dry=0; offset=0; extra=""
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --out) out="${2:?--out needs a folder}"; shift 2 ;;
        --only) only="${2:?--only needs names}"; shift 2 ;;
        --jobs) jobs="${2:?--jobs needs a number}"; shift 2 ;;
        --quick) quick=1; shift ;;
        --dry-run) dry=1; shift ;;
        --seed-offset) offset="${2:?--seed-offset needs a number}"; shift 2 ;;
        --extra) extra="${2:?--extra needs arguments}"; shift 2 ;;
        *) echo "soak_variety: unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
done
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2
out="${out:-/tmp/soak-variety-$(date +%Y%m%d-%H%M)}"
bin=target/release/f4x-tools

if [ "$dry" = 0 ]; then
    if [ -e "$out/summary.txt" ]; then echo "soak_variety: $out/summary.txt exists; use another --out" >&2; exit 2; fi
    mkdir -p "$out" || exit 2
    scripts/cool_run.sh cargo build --release -p f4x-tools || { echo "soak_variety: build failed" >&2; exit 1; }
fi

failed=0
for run in "${RUNS[@]}"; do
    IFS='|' read -r name players map land magic difficulty seeds turns <<<"$run"
    if [ -n "$only" ] && ! [[ ",$only," == *",$name,"* ]]; then continue; fi
    if [ "$quick" = 1 ]; then seeds="${seeds%%..*}..${seeds%%..*}"; turns=300; fi
    seeds="$((${seeds%%..*} + offset))..$((${seeds##*..} + offset))"
    cmd=(scripts/cool_run.sh "$bin" soak --seeds "$seeds" --turns "$turns" --players "$players"
         --map "$map" --land "$land" --magic "$magic" --difficulty "$difficulty"
         --jobs "$jobs" --refused-log "$out/$name-refused.tsv")
    # shellcheck disable=SC2206
    [ -z "$extra" ] || cmd+=($extra)
    if [ "$dry" = 1 ]; then echo "${cmd[*]}"; continue; fi
    echo "soak_variety: $name ($players wizards, $map, land $land%, magic $magic, difficulty $difficulty, seeds $seeds, $turns turns)" >&2
    "${cmd[@]}" >"$out/$name.txt" 2>&1 || { echo "soak_variety: $name failed" >&2; failed=1; }
    games="$(grep -c '^seed [0-9]*: \(won by\|no winner\)' "$out/$name.txt")"
    won="$(grep -c 'won by' "$out/$name.txt")"
    panics="$(grep -c 'panicked' "$out/$name.txt")"
    defeated="$(grep -o '(defeated)' "$out/$name.txt" | wc -l | tr -d ' ')"
    refused="$(($(wc -l <"$out/$name-refused.tsv" 2>/dev/null || echo 1) - 1))"
    printf '%-11s %2s games, %2s won, %2s panics, %2s wizards defeated, %4s refused orders\n' \
        "$name" "$games" "$won" "$panics" "$defeated" "$refused" | tee -a "$out/summary.txt"
    [ "$panics" = 0 ] || failed=1
done
[ "$dry" = 1 ] || echo "soak_variety: reports in $out" >&2
exit "$failed"
