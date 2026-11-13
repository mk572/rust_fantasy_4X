#!/bin/bash
# Overnight variety soaks: plays the runs of scripts/soak_variety.sh again and again on fresh seeds
# until a deadline, writing a detailed log, and watches the laptop's temperature while it goes.
#
#   scripts/soak_overnight.sh --until 07:00 [--out DIR] [--jobs N] [--extra 'SOAK ARGS']
#   scripts/soak_overnight.sh -h | --help
set -u

usage() {
    cat <<'USAGE'
Usage: soak_overnight.sh --until HH:MM [--out DIR] [--jobs N] [--extra 'SOAK ARGS'] [--only LIST]

Plays the variety runs (scripts/soak_variety.sh) round after round, each round on seeds 1000 higher
than the one before, and starts no run after the deadline. Every run goes through cool_run.sh (low
priority, paused while the Mac is warm). A watchdog stops anything still running 10 minutes after
the deadline.

Options:
  --until HH:MM   the local time to stop (the next time the clock shows it)
  --out DIR       folder for everything (default soak-out/overnight-YYYYmmdd-HHMM, git-ignored)
  --jobs N        games played at once in a run (default 4, at most 4 by the project rule)
  --extra ARGS    more arguments for every `f4x-tools soak` (default '--casts --events')
  --only LIST     only these runs of the matrix (comma-separated, see soak_variety.sh --help)

Writes into DIR:
  log.txt         one line for each start and end of a run, with the time, the exit status, the seconds
  thermal.log     every minute: the time, the thermal state (nominal|fair|serious|critical), the load
                  average and the number of soak processes; a state other than nominal is also in log.txt
  r<round>-<run>/ the reports of one run (<run>.txt, <run>-refused.tsv, summary.txt)
  summary-all.txt the games, wins, panics, defeated wizards and refused orders of every run, by name
  panics.txt      every panic line with its game's seed, if any
Exit status: 0 when no run failed or panicked, 1 otherwise, 2 for a usage error.
USAGE
}

until_at=""; out=""; jobs=4; extra="--casts --events"; only=""
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --until) until_at="${2:?--until needs HH:MM}"; shift 2 ;;
        --out) out="${2:?--out needs a folder}"; shift 2 ;;
        --jobs) jobs="${2:?--jobs needs a number}"; shift 2 ;;
        --extra) extra="${2:?--extra needs arguments}"; shift 2 ;;
        --only) only="${2:?--only needs names}"; shift 2 ;;
        *) echo "soak_overnight: unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
done
[ -n "$until_at" ] || { echo "soak_overnight: --until is required" >&2; exit 2; }
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2
now="$(date +%s)"
deadline="$(date -j -f '%H:%M:%S' "$until_at:00" +%s 2>/dev/null)" || { echo "soak_overnight: bad time: $until_at" >&2; exit 2; }
[ "$deadline" -gt "$now" ] || deadline=$((deadline + 86400))
out="${out:-soak-out/overnight-$(date +%Y%m%d-%H%M)}"
mkdir -p "$out" || exit 2
log="$out/log.txt"
stamp() { date '+%Y-%m-%d %H:%M:%S'; }
say() { echo "$(stamp) $*" | tee -a "$log" >&2; }

say "start: until $(date -r "$deadline" '+%Y-%m-%d %H:%M'), jobs $jobs, extra '$extra', out $out"
say "commit: $(git rev-parse --short HEAD) $(git status --short | wc -l | tr -d ' ') changed files"
scripts/cool_run.sh cargo build --release -p f4x-tools || { say "build failed"; exit 1; }

# The thermal watch: once a minute, until the deadline.
(
    last=""
    while [ "$(date +%s)" -lt $((deadline + 900)) ]; do
        state="$(scripts/cool_run.sh --state 2>/dev/null || echo unknown)"
        load="$(uptime | sed 's/.*load averages*: //')"
        procs="$(pgrep -f 'f4x-tools soak' | wc -l | tr -d ' ')"
        echo "$(stamp) thermal $state load $load soak-processes $procs" >>"$out/thermal.log"
        if [ "$state" != "$last" ]; then
            echo "$(stamp) thermal state $state" >>"$log"
            last="$state"
        fi
        sleep 60
    done
) &
watch_pid=$!
# The watchdog: nothing may run on much past the deadline.
(
    sleep $((deadline - $(date +%s) + 600))
    echo "$(stamp) watchdog: deadline passed, stopping every soak" >>"$log"
    pkill -f 'f4x-tools soak' 2>/dev/null
) &
dog_pid=$!
trap 'kill "$watch_pid" "$dog_pid" 2>/dev/null' EXIT

names="small-2p crowded-5p huge-4p islands-3p magic-4p weak-3p hard-3p easy-2p long-2p"
if [ -n "$only" ]; then names="${only//,/ }"; fi
failed=0; round=0
while [ "$(date +%s)" -lt "$deadline" ]; do
    for name in $names; do
        [ "$(date +%s)" -lt "$deadline" ] || break 2
        dir="$out/r$round-$name"
        say "run r$round $name: start (seed offset $((round * 1000)))"
        began="$(date +%s)"
        scripts/soak_variety.sh --out "$dir" --only "$name" --jobs "$jobs" --seed-offset $((round * 1000)) --extra "$extra" >"$dir.stdout" 2>&1
        code=$?
        [ "$code" = 0 ] || failed=1
        say "run r$round $name: end, exit $code, $(($(date +%s) - began))s, $(cat "$dir/summary.txt" 2>/dev/null | tr -s ' ')"
    done
    round=$((round + 1))
done
say "stopped: $round rounds started"

# The totals.
: >"$out/summary-all.txt"
for dir in "$out"/r*-*/; do
    [ -f "$dir/summary.txt" ] || continue
    echo "$(basename "$dir"): $(tr -s ' ' <"$dir/summary.txt")" >>"$out/summary-all.txt"
done
: >"$out/panics.txt"
for report in "$out"/r*-*/*.txt; do
    case "$report" in */summary.txt) continue ;; esac
    grep -H -B1 'panicked' "$report" >>"$out/panics.txt" 2>/dev/null
done
[ -s "$out/panics.txt" ] && failed=1
say "summary: $out/summary-all.txt, panics: $(grep -c panicked "$out/panics.txt") lines"
exit "$failed"
