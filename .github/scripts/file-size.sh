#!/usr/bin/env bash
# Fails when a hand-written code file is over 1000 lines. A file that has to
# stay longer says why on a line reading `file-size-guard: <why>` near its top.
set -euo pipefail
limit=1000
bad=0
while IFS= read -r f; do
  n=$(wc -l < "$f" | tr -d " ")
  if [ "$n" -gt "$limit" ] && ! head -n 40 "$f" | grep -q 'file-size-guard:'; then
    echo "::error file=$f::$f is $n lines, over $limit. Split it along a seam, each test moving with its code, or say why on a 'file-size-guard: <why>' line near its top."
    bad=1
  fi
done < <(git ls-files -- '*.rs' '*.ts' '*.tsx' '*.js' '*.mjs' '*.py' '*.sh')
exit "$bad"
