# `zootr live` reads the running game's memory, which macOS only allows a
# binary signed with the debugger entitlement. Signing redoes on every build.

ZOOTR := target/debug/zootr

.PHONY: zootr live

zootr:
	cargo build -p zootr
	codesign -f -s - --entitlements crates/zootr/live.entitlements $(ZOOTR)

# make live ARGS="--grep Rupee"
live: zootr
	$(ZOOTR) live $(ARGS)

dev-app:
	cd app && pnpm install && pnpm tauri dev