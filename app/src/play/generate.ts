import type { KeyboardEvent } from "react";
import { generate, useSeeds } from "@/stores/seeds";
import { fix } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** Generates from the typed seed, installing it or only adding it to
 * History. When `blocked` says why it can't, a click opens where that's
 * fixed and a key does nothing. */
export function generateTyped(install: boolean, blocked: string, click = true) {
  if (!blocked) void generate(useUi.getPath("newSeed.seed") || null, install);
  else if (click) fix(blocked);
}

/** ⌘↵ generates and installs, ⇧⌘↵ only generates: the Standard folder's
 * keydown. */
export function generateKey(e: KeyboardEvent) {
  if (e.key !== "Enter" || !(e.metaKey || e.ctrlKey)) return;
  const { blocked, unbuildable, busy } = useSeeds.getState();
  if (busy) return;
  e.preventDefault();
  if (e.shiftKey) generateTyped(false, unbuildable, false);
  else generateTyped(true, blocked, false);
}
