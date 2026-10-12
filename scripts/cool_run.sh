#!/bin/bash
# Run a long command (training, soak runs) politely on a laptop: low priority, and paused while the
# Mac is under thermal pressure, so it never heats the machine into the slow-down that makes
# everything crawl.
#
#   scripts/cool_run.sh COMMAND [ARGS...]
#   scripts/cool_run.sh --state          print nominal | fair | serious | critical | unknown
#   scripts/cool_run.sh -h | --help      print the usage
#
# How it works:
#   * the command runs under `taskpolicy -b` (background QoS: the efficiency cores, low I/O
#     priority) and `nice -n 19`;
#   * every COOL_POLL seconds the thermal state is read (ProcessInfo.thermalState, no privileges;
#     the same signal the imagegen project uses);
#   * when the state is COOL_PAUSE_AT or worse, the whole command is stopped (SIGSTOP) and only
#     continued (SIGCONT) once the state is back to nominal, then it rests COOL_REST more seconds;
#   * a state that cannot be read counts as hot.
# Environment (defaults): COOL_PAUSE_AT=fair  COOL_POLL=15  COOL_REST=60
# The command should also limit its own threads (the training tasks use at most 4 of the 10 cores).
set -u
usage() {
    cat <<'USAGE'
Usage: cool_run.sh COMMAND [ARGS...]
       cool_run.sh --state
       cool_run.sh -h | --help

Run a long command (training, soak runs) politely on a laptop: at low priority, and paused
while the Mac is under thermal pressure, so it never heats the machine into the slow-down that
makes everything crawl.

Options:
  --state       print the thermal state (nominal | fair | serious | critical | unknown) and exit
  -h, --help    print this help and exit

COMMAND [ARGS...] is run under `taskpolicy -b` and `nice -n 19`; its exit status is the
script's. The command is stopped (SIGSTOP) when the thermal state is COOL_PAUSE_AT or worse and
continued once the state is nominal again, after COOL_REST more seconds. A state that cannot be
read counts as hot. The first word must be a command, not an option.

Environment (defaults): COOL_PAUSE_AT=fair  COOL_POLL=15 (seconds between readings)
  COOL_REST=60 (seconds of rest before continuing)  COOL_STATE_BIN=scripts/thermal_state

Needs: swiftc (builds scripts/thermal_state from thermal_state.swift when it is missing).
Writes: progress lines ("cool_run: ...") to stderr. Exit status 2 for a usage error.
USAGE
}
case "${1:-}" in
    -h|--help) usage; exit 0 ;;
    --state|"") ;;
    -*) echo "cool_run: unknown option: $1 (see --help)" >&2; exit 2 ;;
esac
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${COOL_STATE_BIN:-$here/thermal_state}"
if [ ! -x "$bin" ]; then
    swiftc -O "$here/thermal_state.swift" -o "$bin" || { echo "cool_run: cannot build thermal_state" >&2; exit 2; }
fi
state() { "$bin" 2>/dev/null || echo unknown; }
rank() { case "$1" in nominal) echo 0;; fair) echo 1;; serious) echo 2;; critical) echo 3;; *) echo 9;; esac; }
if [ "${1:-}" = "--state" ]; then state; exit 0; fi
[ $# -ge 1 ] || { echo "cool_run: no command given (see --help)" >&2; exit 2; }

pause_at="$(rank "${COOL_PAUSE_AT:-fair}")"
poll="${COOL_POLL:-15}"
rest="${COOL_REST:-60}"
log() { echo "cool_run: $(date '+%F %T') $*" >&2; }

# wait before starting, too
while [ "$(rank "$(state)")" -ge "$pause_at" ]; do log "not cool ($(state)), waiting before start"; sleep "$poll"; done

set -m
taskpolicy -b nice -n 19 "$@" &
pid=$!
trap 'kill -CONT -- -$pid 2>/dev/null; kill -TERM -- -$pid 2>/dev/null; exit 130' INT TERM

paused=0
while kill -0 "$pid" 2>/dev/null; do
    st="$(state)"; r="$(rank "$st")"
    if [ "$paused" = 0 ] && [ "$r" -ge "$pause_at" ]; then
        kill -STOP -- -$pid 2>/dev/null; paused=1; log "paused: thermal state $st"
    elif [ "$paused" = 1 ] && [ "$r" -eq 0 ]; then
        log "nominal again, resting ${rest}s"; sleep "$rest"
        if [ "$(rank "$(state)")" -eq 0 ]; then kill -CONT -- -$pid 2>/dev/null; paused=0; log "resumed"; fi
    fi
    sleep "$poll"
done
wait "$pid"
