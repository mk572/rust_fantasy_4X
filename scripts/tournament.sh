#!/bin/bash
# Battle AI tournament, neural against neural (docs/PLANNED.md item 25): picks parents from
# checkpoints by a round robin, makes variants of them (nudged biases and noise), then plays a
# Swiss stage and a knockout. Runs politely (scripts/cool_run.sh) into one folder.
#
#   scripts/tournament.sh [options]        see --help
set -u

usage() {
    cat <<'USAGE'
Usage: tournament.sh [--name NAME] (--checkpoints LIST | --entries FILE) [options] [--resume]
       tournament.sh -h | --help

Run a tournament of trained battle networks, neural against neural, with nobody watching.

Stages (the first two are skipped with --entries):
  1. rank      the candidates from --checkpoints play a round robin; the best --parents become parents
  2. variants  each parent gets --auto variants: random nudges of its output biases (more or fewer
               spells, melee, shots, walking, being done, fleeing, walls, big casts) and/or noise
  3. run       a Swiss stage (--swiss-rounds rounds of --swiss-pairs pairs a match), then the top
               --ko entries play single elimination (--ko-pairs pairs a match)
A pair is one scenario played twice with the sides swapped; ties in a pair go to enemy units
killed, own units left, hit points left, less mana spent; a level pair is a draw.

Options:
  --name NAME           folder models-local/tournament-NAME (default YYYYmmdd-HHMM); an existing
                        folder is never overwritten, the script stops instead
  --checkpoints LIST    candidates: comma-separated .bpk files and/or run folders (a folder gives
                        its net_NNNN.bpk files, every --every'th one)
  --every N             with a run folder: take every Nth checkpoint (default 2)
  --entries FILE        use this entries file (name<TAB>checkpoint<TAB>parent<TAB>note) and skip
                        stages 1 and 2
  --parents K           parents to pick (default 3)
  --rank-pairs N        pairs per match in the round robin (default 15)
  --auto N              variants per parent (default 10)
  --strengths LIST      sizes of the nudges, comma-separated (default 1,2,4)
  --noise LIST          sizes of the noise, comma-separated (default 0.01,0.03,0.1)
  --swiss-rounds N      Swiss rounds (default 6)
  --swiss-pairs N       pairs per Swiss match (default 15)
  --ko N                entries in the knockout, a power of two (default 8)
  --ko-pairs N          pairs per knockout match (default 101)
  --vs-scripted N       battles each knockout entry plays against the scripted AI at the end, for
                        information only (default 300; 0 skips)
  --step-limit N        commands after which a battle counts as having no winner (default 3000)
  --seed N              seed of the tournament (default 1)
  --bank FILE           scenario bank (default models-local/bank-final.ron; make a fresh one with
                        `cargo run --release -p f4x-learn --example make_bank -- FILE 1200 80000000`)
  --threads N           threads (default 4; the M4 has 10 cores)
  --detach              run in the background (nohup, and caffeinate so the Mac does not idle-sleep)
                        and return at once; follow it with the files below
  --notify              show a macOS notification when the run ends
  --resume              continue the run of --name in its existing folder: stages already done are
                        skipped and finished matches are kept
  --dry-run             print what would run and exit
  -h, --help            print this help and exit

Files in the folder:
  STATUS         running (stage) | done | failed (what) | interrupted, and when
  command.txt    the options, the git commit and the date the run started
  candidates.txt the candidates;  rank/  the round robin (standings.txt, parents.txt)
  variants/      the parents and variants as checkpoints, and entries.txt with their recipes
  run/           log.txt, matches/ (every match in full, as .ron and .txt), standings-swiss.txt,
                 bracket.txt, standings.txt (rating over every match), profiles.txt (what each
                 entry did per battle), deciders.txt (what settled the pairs), vs_scripted.txt,
                 winner.bpk with winner.shape.ron
  summary.txt    the champion and the bracket
A stopped run is continued with --resume and the same --name and options.

Needs: cargo, scripts/cool_run.sh, and the bank. Exit status: 0 on success; 2 for a usage error or
a folder that exists; otherwise the failing stage's.
USAGE
}

# Everything runs inside main, so bash has read the whole script before it starts and editing the
# file during a long run cannot break the run.
main() {
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
cd "$repo" || exit 2

name="$(date +%Y%m%d-%H%M)"
bank="models-local/bank-final.ron"
checkpoints="" entries="" every=2 parents=3 rank_pairs=15 auto=10
strengths="1,2,4" noise="0.01,0.03,0.1"
swiss_rounds=6 swiss_pairs=15 ko=8 ko_pairs=101 vs_scripted=300 step_limit=3000 seed=1
threads=4 detach=0 notify=0 dry=0 resume=0
args=("$@")
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --name) name="${2:?--name needs a value}"; shift ;;
        --checkpoints) checkpoints="${2:?--checkpoints needs a value}"; shift ;;
        --every) every="${2:?--every needs a value}"; shift ;;
        --entries) entries="${2:?--entries needs a value}"; shift ;;
        --parents) parents="${2:?--parents needs a value}"; shift ;;
        --rank-pairs) rank_pairs="${2:?--rank-pairs needs a value}"; shift ;;
        --auto) auto="${2:?--auto needs a value}"; shift ;;
        --strengths) strengths="${2:?--strengths needs a value}"; shift ;;
        --noise) noise="${2:?--noise needs a value}"; shift ;;
        --swiss-rounds) swiss_rounds="${2:?--swiss-rounds needs a value}"; shift ;;
        --swiss-pairs) swiss_pairs="${2:?--swiss-pairs needs a value}"; shift ;;
        --ko) ko="${2:?--ko needs a value}"; shift ;;
        --ko-pairs) ko_pairs="${2:?--ko-pairs needs a value}"; shift ;;
        --vs-scripted) vs_scripted="${2:?--vs-scripted needs a value}"; shift ;;
        --step-limit) step_limit="${2:?--step-limit needs a value}"; shift ;;
        --seed) seed="${2:?--seed needs a value}"; shift ;;
        --bank) bank="${2:?--bank needs a value}"; shift ;;
        --threads) threads="${2:?--threads needs a value}"; shift ;;
        --detach) detach=1 ;;
        --notify) notify=1 ;;
        --dry-run) dry=1 ;;
        --resume) resume=1 ;;
        *) echo "tournament.sh: unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done

if [ -z "$checkpoints" ] && [ -z "$entries" ]; then
    echo "tournament.sh: give --checkpoints LIST or --entries FILE (see --help)" >&2
    exit 2
fi
if [ -n "$entries" ] && [ ! -f "$entries" ]; then
    echo "tournament.sh: no entries file at $entries" >&2
    exit 2
fi
out="models-local/tournament-$name"
if [ -z "${F4X_TRAIN_CHILD:-}" ] && [ "$resume" = 0 ] && [ -e "$out" ]; then
    echo "tournament.sh: $out exists; pick another --name, or --resume to continue it" >&2
    exit 2
fi
[ -f "$bank" ] || { echo "tournament.sh: no bank at $bank (see --help for how to make one)" >&2; exit 2; }

if [ "$dry" = 1 ]; then
    echo "folder:  $out"
    if [ -n "$entries" ]; then
        echo "entries: $entries (stages 1 and 2 skipped)"
    else
        echo "stage 1: round robin of the candidates in '$checkpoints' (every $every), $rank_pairs pairs a match, best $parents become parents"
        echo "stage 2: $auto variants per parent (nudges $strengths, noise $noise)"
    fi
    echo "stage 3: Swiss $swiss_rounds rounds x $swiss_pairs pairs, knockout of $ko x $ko_pairs pairs, vs scripted $vs_scripted"
    echo "bank: $bank   threads: $threads   seed: $seed   step limit: $step_limit   detach: $detach   notify: $notify"
    exit 0
fi

# Background: start this script again, detached, and leave.
if [ "$detach" = 1 ] && [ -z "${F4X_TRAIN_CHILD:-}" ]; then
    mkdir -p "$out"
    child_args=()
    for a in ${args[@]+"${args[@]}"}; do [ "$a" = "--detach" ] || child_args+=("$a"); done
    F4X_TRAIN_CHILD=1 nohup caffeinate -i "$here/tournament.sh" ${child_args[@]+"${child_args[@]}"} \
        >"$out/script.txt" 2>&1 &
    echo "started in the background (pid $!); folder $out"
    echo "follow it: cat $out/STATUS; tail -f $out/run/log.txt"
    exit 0
fi

mkdir -p "$out"
status() { echo "$1 ($(date '+%Y-%m-%d %H:%M:%S'))" >"$out/STATUS"; }
finish() {
    local what="$1"
    status "$what"
    if [ "$notify" = 1 ]; then
        osascript -e "display notification \"$name: $what\" with title \"Battle AI tournament\"" >/dev/null 2>&1 || true
    fi
}
trap 'finish interrupted; exit 130' INT TERM

{
    echo "options: ${args[*]-}"
    echo "bank: $bank   threads: $threads"
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null) $(git status --porcelain | wc -l | tr -d ' ') changed files"
    echo "started: $(date '+%Y-%m-%d %H:%M:%S')"
} >"$out/command.txt"
status "running (build)"

if ! scripts/cool_run.sh cargo build --release -p f4x-learn --features train \
        --example tournament --example make_variants >"$out/build.txt" 2>&1; then
    finish "failed (build; see $out/build.txt)"
    exit 1
fi
tour=target/release/examples/tournament
mkv=target/release/examples/make_variants
run_tool() { RAYON_NUM_THREADS="$threads" scripts/cool_run.sh "$@"; }

if [ -z "$entries" ] && [ -f "$out/variants/entries.txt" ]; then
    entries="$out/variants/entries.txt"   # resumed after stage 2
fi
if [ -z "$entries" ]; then
    # Stage 1: the candidates, from files and run folders.
    : >"$out/candidates.txt"
    IFS=',' read -r -a items <<<"$checkpoints"
    for item in "${items[@]}"; do
        if [ -d "$item" ]; then
            tag="${item%/}"; tag="${tag##*-}"
            n=0
            for f in $(ls "$item"/net_*.bpk 2>/dev/null | sort); do
                n=$((n + 1))
                [ $(( (n - 1) % every )) -eq 0 ] || continue
                num="${f##*net_}"; num="${num%.bpk}"
                printf '%s\t%s\t-\tcandidate from %s\n' "$tag-$num" "$f" "$item" >>"$out/candidates.txt"
            done
        elif [ -f "$item" ]; then
            base="${item##*/}"; base="${base%.bpk}"
            printf '%s\t%s\t-\tcandidate\n' "$base" "$item" >>"$out/candidates.txt"
        else
            finish "failed (no such checkpoint or folder: $item)"
            exit 2
        fi
    done
    if [ "$(wc -l <"$out/candidates.txt")" -lt 2 ]; then
        finish "failed (fewer than two candidates in $out/candidates.txt)"
        exit 2
    fi
    status "running (stage 1: round robin of $(wc -l <"$out/candidates.txt" | tr -d ' ') candidates)"
    [ -f "$out/rank/parents.txt" ] || run_tool "$tour" rank "$out/rank" --entries "$out/candidates.txt" --pairs "$rank_pairs" \
        --top "$parents" --bank "$bank" --seed "$seed" --step-limit "$step_limit" \
        >"$out/rank-stdout.txt" 2>&1 || { finish "failed (stage 1; see $out/rank-stdout.txt)"; exit 1; }

    # Stage 2: variants of the parents.
    status "running (stage 2: variants)"
    parent_opts=()
    while IFS=$'\t' read -r pname ppath _rest; do
        parent_opts+=(--parent "$pname=$ppath")
    done <"$out/rank/parents.txt"
    run_tool "$mkv" "$out/variants" ${parent_opts[@]+"${parent_opts[@]}"} --auto "$auto" \
        --strengths "$strengths" --noise "$noise" --seed "$seed" \
        >"$out/variants-stdout.txt" 2>&1 || { finish "failed (stage 2; see $out/variants-stdout.txt)"; exit 1; }
    entries="$out/variants/entries.txt"
fi

# Stage 3: Swiss stage and knockout.
status "running (stage 3: Swiss stage and knockout)"
run_tool "$tour" run "$out/run" --entries "$entries" --swiss-rounds "$swiss_rounds" \
    --swiss-pairs "$swiss_pairs" --ko "$ko" --ko-pairs "$ko_pairs" --vs-scripted "$vs_scripted" \
    --bank "$bank" --seed "$seed" --step-limit "$step_limit" \
    >"$out/run-stdout.txt" 2>&1
code=$?
[ -f "$out/run/summary.txt" ] && cp "$out/run/summary.txt" "$out/summary.txt"
if [ "$code" -eq 0 ]; then finish done; else finish "failed (stage 3, exit code $code; see $out/run-stdout.txt)"; fi
exit "$code"
}
main "$@"
exit $?
