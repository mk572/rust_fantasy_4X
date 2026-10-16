#!/bin/bash
# Train the battle AI without supervision (task 170/172): builds the trainer, runs it politely
# (scripts/cool_run.sh: low priority, paused while the Mac is hot), keeps every output in one run
# folder, picks the best checkpoint by its held-out score and writes a summary.
#
#   scripts/train_battle.sh [options]        see --help
set -u

usage() {
    cat <<'USAGE'
Usage: train_battle.sh [--name NAME] [--iters N] [--bc N] [--battles N] [--seed N]
                       [--resume FILE] [--start-iter N]
                       [--no-advice] [--no-briefs] [--until WHEN] [--tz ZONE] [--bank FILE]
                       [--threads N]
                       [--detach] [--notify] [--dry-run]
       train_battle.sh -h | --help

Train the battle AI (f4x-learn's train_battle example) from start to finish with nobody
watching: build, train through scripts/cool_run.sh, choose the best checkpoint, write a summary.

Options:
  --name NAME     run folder models-local/NAME (default run-YYYYmmdd-HHMM); an existing folder
                  is never overwritten, the script stops instead
  --iters N       PPO iterations (default 50)
  --bc N          scripted battles for the warm start, 0 to skip it (default 200)
  --battles N     battles per iteration (default 64)
  --seed N        seed of the run (default 1)
  --resume FILE   start from a checkpoint's weights (a .bpk with its .shape.ron; the optimiser
                  starts afresh and the warm start is skipped); with --start-iter N the
                  iterations are numbered from N + 1
  --start-iter N  see --resume
  --no-advice     never show the scripted AI's advice (the control run)
  --no-briefs     train without random briefs
  --until WHEN    stop at a time: HH:MM today, or "YYYY-MM-DD HH:MM", in --tz (default
                  Asia/Tokyo); the count of iterations is then only a ceiling (100000 unless
                  --iters is given). The last iteration is the one after which another would end
                  past the time, so the final evaluations finish within a few minutes of it
  --tz ZONE       the time zone of --until (default Asia/Tokyo)
  --bank FILE     scenario bank (default models-local/bank-v2.ron)
  --threads N     threads for the trainer (default 4; the M4 has 10 cores)
  --detach        run in the background (nohup, and caffeinate so the Mac does not idle-sleep)
                  and return at once; follow it with the files below
  --notify        show a macOS notification when the run ends
  --dry-run       print what would run and exit
  -h, --help      print this help and exit

Files in the run folder:
  STATUS          running | done | failed (exit code) | interrupted, and when
  command.txt     the options, the git commit and the date the run started
  build.txt       the build's output;  stdout.txt  the trainer's output
  log.txt         the trainer's own log (one line per iteration, evals every 5 iterations,
                  with the casts and mana of each evaluation)
  casts.txt       each evaluation's casts by spell, in full
  net_NNNN.bpk    a checkpoint every 5 iterations, each with net_NNNN.shape.ron
  best.bpk        a copy of the checkpoint with the best "advice hidden" held-out reward
                  (with best.shape.ron), and summary.txt saying which and how it scored

Needs: cargo, scripts/cool_run.sh (builds its thermal tool with swiftc when missing), and the
bank (make one with `cargo run --release -p f4x-learn --example make_bank`).
Exit status: the trainer's; 2 for a usage error or a run folder that exists.
USAGE
}

# Everything runs inside main, so bash has read the whole script before it starts and editing the
# file during a long run cannot break the run.
main() {
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
cd "$repo" || exit 2

name="run-$(date +%Y%m%d-%H%M)"
bank="models-local/bank-v2.ron"
threads=4
detach=0 notify=0 dry=0
until="" tz="Asia/Tokyo"
train_opts=()
args=("$@")
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --name) name="${2:?--name needs a value}"; shift ;;
        --iters|--bc|--battles|--seed|--resume|--start-iter) train_opts+=("$1" "${2:?$1 needs a value}"); shift ;;
        --no-advice|--no-briefs) train_opts+=("$1") ;;
        --until) until="${2:?--until needs a value}"; shift ;;
        --tz) tz="${2:?--tz needs a value}"; shift ;;
        --bank) bank="${2:?--bank needs a value}"; shift ;;
        --threads) threads="${2:?--threads needs a value}"; shift ;;
        --detach) detach=1 ;;
        --notify) notify=1 ;;
        --dry-run) dry=1 ;;
        *) echo "train_battle.sh: unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done

# A deadline: HH:MM today or "YYYY-MM-DD HH:MM", in the time zone TZ; the trainer stops after the
# last iteration that would end before it (and its evaluations), however many that is.
if [ -n "$until" ]; then
    case "$until" in
        [0-9][0-9]:[0-9][0-9]) until="$(TZ="$tz" date +%Y-%m-%d) $until" ;;
    esac
    until_epoch=$(TZ="$tz" date -j -f "%Y-%m-%d %H:%M:%S" "$until:00" +%s 2>/dev/null) \
        || { echo "train_battle.sh: cannot read --until '$until' (use HH:MM or 'YYYY-MM-DD HH:MM')" >&2; exit 2; }
    [ "$until_epoch" -gt "$(date +%s)" ] \
        || { echo "train_battle.sh: --until '$until' $tz is already past" >&2; exit 2; }
    case " ${train_opts[*]-} " in
        *" --iters "*) ;;
        *) train_opts+=(--iters 100000) ;;
    esac
    train_opts+=(--until "$until_epoch")
fi

out="models-local/$name"
if [ -z "${F4X_TRAIN_CHILD:-}" ] && [ -e "$out" ]; then
    echo "train_battle.sh: $out exists; pick another --name (nothing is overwritten)" >&2
    exit 2
fi
[ -f "$bank" ] || { echo "train_battle.sh: no bank at $bank (make one with the make_bank example)" >&2; exit 2; }

trainer=(target/release/examples/train_battle "$out" ${train_opts[@]+"${train_opts[@]}"})
if [ "$dry" = 1 ]; then
    echo "run folder: $out"
    echo "build:      scripts/cool_run.sh cargo build --release -p f4x-learn --features train --example train_battle"
    echo "train:      RAYON_NUM_THREADS=$threads BANK=$bank scripts/cool_run.sh ${trainer[*]}"
    echo "detach:     $detach   notify: $notify   until: ${until:-(none)} ${until_epoch:+(epoch $until_epoch)}"
    exit 0
fi

# Background: start this script again, detached, and leave.
if [ "$detach" = 1 ] && [ -z "${F4X_TRAIN_CHILD:-}" ]; then
    mkdir -p "$out"
    child_args=()
    for a in ${args[@]+"${args[@]}"}; do [ "$a" = "--detach" ] || child_args+=("$a"); done
    F4X_TRAIN_CHILD=1 nohup caffeinate -i "$here/train_battle.sh" ${child_args[@]+"${child_args[@]}"} \
        >"$out/script.txt" 2>&1 &
    echo "started in the background (pid $!); folder $out"
    echo "follow it: cat $out/STATUS; tail -f $out/log.txt"
    exit 0
fi

mkdir -p "$out"
status() { echo "$1 ($(date '+%Y-%m-%d %H:%M:%S'))" >"$out/STATUS"; }
finish() {
    local what="$1"
    status "$what"
    if [ "$notify" = 1 ]; then
        osascript -e "display notification \"$name: $what\" with title \"Battle AI training\"" >/dev/null 2>&1 || true
    fi
}
trap 'finish interrupted; exit 130' INT TERM

{
    echo "options: ${args[*]-}"
    echo "bank: $bank   threads: $threads"
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null) $(git status --porcelain | wc -l | tr -d ' ') changed files"
    echo "started: $(date '+%Y-%m-%d %H:%M:%S')"
} >"$out/command.txt"
status running

if ! scripts/cool_run.sh cargo build --release -p f4x-learn --features train --example train_battle \
        >"$out/build.txt" 2>&1; then
    finish "failed (build; see $out/build.txt)"
    exit 1
fi

RAYON_NUM_THREADS="$threads" BANK="$bank" scripts/cool_run.sh "${trainer[@]}" >"$out/stdout.txt" 2>&1
code=$?

# The best checkpoint: the highest held-out reward with the advice hidden (the network on its
# own), from the "eval N (advice hidden): ... reward R;" lines of the log.
best=$(sed -n 's/^eval \([0-9]*\) (advice hidden):.*reward \(-\{0,1\}[0-9.]*\);.*/\2 \1/p' "$out/log.txt" 2>/dev/null \
    | sort -g -r | head -1)
{
    echo "run: $name   trainer exit code: $code"
    if [ -n "$best" ]; then
        score="${best% *}"; it="${best#* }"
        file=$(printf '%s/net_%04d' "$out" "$it")
        echo "best: iteration $it, held-out reward $score (advice hidden) -> $file.bpk"
        if [ -f "$file.bpk" ]; then
            cp "$file.bpk" "$out/best.bpk"
            [ -f "$file.shape.ron" ] && cp "$file.shape.ron" "$out/best.shape.ron"
        else
            echo "(that checkpoint file is missing)"
        fi
    else
        echo "no evaluation line in the log: no best checkpoint chosen"
    fi
    echo
    grep -E '^(baseline|start|eval)' "$out/log.txt" 2>/dev/null
} >"$out/summary.txt"

if [ "$code" -eq 0 ]; then finish done; else finish "failed (trainer exit code $code)"; fi
exit "$code"
}
main "$@"
exit $?
