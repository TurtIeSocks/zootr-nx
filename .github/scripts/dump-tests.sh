#!/bin/bash
# The zootr tests with the game dump, on the self-hosted runner the "Dump
# tests" job asks for. The runner mounts the dump at /dump, a cache at
# /cache and a log folder at /logs. The public log shows only the result
# lines and failing test names: the full output quotes game text, so it
# stays in /logs on the runner's host.
set -uo pipefail

if [ ! -f /dump/oot.nup ]; then
  echo "no dump at /dump/oot.nup: refusing to run the tests without it"
  exit 1
fi

# One build folder a job. The box runs two jobs at once, and a shared
# folder let one job's build replace another's: its build script, baked
# with a deleted checkout's path, or a test binary another job then ran.
# A job claims a free folder with flock and holds it until it exits (the
# lock goes with the process, even on a cancel), so no two jobs ever share
# one. With every folder taken it waits for the first, never sharing.
for slot in 0 1; do
  exec 9>"/cache/target-$slot.lock"
  flock -n 9 && break
  exec 9>&-
  slot=
done
if [ -z "$slot" ]; then
  slot=0
  exec 9>"/cache/target-$slot.lock"
  echo "both build folders busy, waiting for one"
  flock 9
fi
export CARGO_TARGET_DIR=/cache/target-$slot

# The build folder grows with every toolchain and dependency bump.
if [ -d "$CARGO_TARGET_DIR" ] && [ "$(du -s --block-size=1G "$CARGO_TARGET_DIR" | cut -f1)" -gt 10 ]; then
  echo "build folder over 10 GB, wiping it"
  rm -rf "$CARGO_TARGET_DIR"
fi

# Build from a fixed copy of the checkout, one a build folder. Each job
# checks out to a new path with every file new, so cargo rebuilt all our
# crates and rustc's incremental cache never matched. rsync --checksum
# without -t rewrites only the files whose content changed, so the rest
# keep their old times. Then only changed crates rebuild, and rustc reuses
# the unchanged parts of zootr: a one-line edit built in 44 s against
# 173 s. --delete drops files the checkout no longer has.
src=/cache/src-$slot
mkdir -p "$src"
rsync -rlpc --delete --exclude .git ./ "$src/"
cd "$src" || exit 1

# The tests open ../../dump/oot.nup from the crate.
ln -sf /dump/oot.nup dump/oot.nup

log=/logs/$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.log
# The nightly run adds the checks too slow for every PR.
[ "${GITHUB_EVENT_NAME:-}" = schedule ] && export ZOOTR_NIGHTLY=1
# Every run but a PR's (main, the nightly, the release PR's dispatch) adds
# the checks that only need to catch up after a merge: the committed
# derives against fresh ones, about 470 s.
[ "${GITHUB_EVENT_NAME:-}" != pull_request ] && export ZOOTR_AFTER_MERGE=1
export CARGO_TERM_COLOR=never
# Two jobs share a box, so each runs its tests on its slot's half of the
# cores. The test threads and par_map follow the CPU affinity, so a job
# runs half as much at once: on 4 of 8 cores the suite peaked at 4.5 GB
# against 6.6 GB on all 8, in about the same time, which a few long tests
# set. Two at 6.6 GB OOM-killed each other on 15 GB. The build uses
# every core.
half=$(($(nproc) / 2))
cores=$((slot * half))-$((slot * half + half - 1))
# A PR runs only the guards that keep the game's names out of this public
# repo: a name merged to main is published. The whole suite runs on main
# after the merge, and nightly. Over 300 PR runs it caught only two
# problems the other jobs missed, one of them a name, while each PR waited
# 11 to 14 minutes for it.
targets=(--lib --bins --tests)
filters=()
if [ "${GITHUB_EVENT_NAME:-}" = pull_request ]; then
  targets=(--lib)
  filters=(-- known::guard ap::export::tests::the_export_spells_no_internal_id)
  echo "a PR: only the name guards, the whole suite runs on main"
fi
cargo test -p zootr "${targets[@]}" --no-run > "$log" 2>&1 &&
  taskset -c "$cores" cargo test -p zootr "${targets[@]}" "${filters[@]}" >> "$log" 2>&1
status=$?

grep -E '^test result:|^test .* \.\.\. FAILED$' "$log"
grep -q '^error: could not compile' "$log" && echo "build failed"
echo "full log on the runner's host: $(basename "$log")"
exit $status
