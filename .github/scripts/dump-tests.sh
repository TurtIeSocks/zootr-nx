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

log=/logs/$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.log
CARGO_TERM_COLOR=never cargo test -p zootr --lib --bins --tests > "$log" 2>&1
status=$?
# The slow checks the default run skips: the AP export against each pair
# of options' seeds. They run from the lib test binary the run above
# built, not a second cargo run: jobs share the cache, and between two
# cargo runs another job can swap in its own build script, pointing at its
# checkout. One cargo run a job leaves no gap for that.
bin=$(sed -n 's|^ *Running unittests src/lib.rs (\(.*\))$|\1|p' "$log" | head -1)
if [ -x "$bin" ]; then
  (cd crates/zootr && "$bin" --ignored ap::export::) >> "$log" 2>&1 || status=1
else
  echo "no lib test binary in the log, so the slow AP checks didn't run"
  status=1
fi

grep -E '^test result:|^test .* \.\.\. FAILED$' "$log"
grep -q '^error: could not compile' "$log" && echo "build failed"
echo "full log on the runner's host: $(basename "$log")"
exit $status
