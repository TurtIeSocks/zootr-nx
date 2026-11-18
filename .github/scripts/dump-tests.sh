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

# The build cache grows with every toolchain and dependency bump.
if [ -d "$CARGO_TARGET_DIR" ] && [ "$(du -s --block-size=1G "$CARGO_TARGET_DIR" | cut -f1)" -gt 15 ]; then
  echo "build cache over 15 GB, wiping it"
  rm -rf "$CARGO_TARGET_DIR"
fi

# The tests open ../../dump/oot.nup from the crate.
ln -sf /dump/oot.nup dump/oot.nup

# The dev profile leaves our own crate unoptimised, and the logic and fill
# code the tests run spend most of their CPU in it, hashing above all. Level
# 1 costs about 25 s more to compile and runs the tests in about half the
# CPU. Debug assertions and overflow checks stay on. Local builds keep 0.
opt=(--config 'profile.dev.package.zootr.opt-level=1')

log=/logs/$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.log
CARGO_TERM_COLOR=never cargo test "${opt[@]}" -p zootr --lib --bins --tests > "$log" 2>&1
status=$?

grep -E '^test result:|^test .* \.\.\. FAILED$' "$log"
grep -q '^error: could not compile' "$log" && echo "build failed"
echo "full log on the runner's host: $(basename "$log")"
exit $status
