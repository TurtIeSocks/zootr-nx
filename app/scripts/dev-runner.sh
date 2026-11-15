#!/bin/sh
# Cargo for `tauri dev`, signing the debug app for live reads before it runs.
# Tauri calls this as `run <cargo args> -- <app args>`. Anything else, or any
# other OS, goes straight to cargo.
set -e
root=$(cd "$(dirname "$0")/../.." && pwd)
if [ "$1" != run ] || [ "$(uname)" != Darwin ]; then
  exec cargo "$@"
fi
shift
build=""
while [ $# -gt 0 ] && [ "$1" != -- ]; do
  build="$build $1"
  shift
done
[ "$1" = -- ] && shift
# shellcheck disable=SC2086
cargo build $build
app="${CARGO_TARGET_DIR:-$root/target}/debug/zootr-app"
codesign -f -s - --entitlements "$root/crates/zootr/live.entitlements" "$app"
exec "$app" "$@"
