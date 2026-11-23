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

# The tests open ../../dump/oot.nup from the crate.
ln -sf /dump/oot.nup dump/oot.nup

log=/logs/$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.log
# CI is set, so the run includes the slow AP export checks.
CARGO_TERM_COLOR=never cargo test -p zootr --lib --bins --tests > "$log" 2>&1
status=$?

grep -E '^test result:|^test .* \.\.\. FAILED$' "$log"
grep -q '^error: could not compile' "$log" && echo "build failed"
echo "full log on the runner's host: $(basename "$log")"
exit $status
