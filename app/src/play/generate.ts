import type { KeyboardEvent } from "react";
import { generate, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

/** Generates from the typed seed, installing it or only adding it to
 * History. Does nothing when `blocked` says why it can't: a blocked
 * button opens where that's fixed itself (WhyButton's `onWhy`). */
export function generateTyped(install: boolean, blocked: string) {
  if (!blocked) void generate(useUi.getPath("newSeed.seed") || null, install);
}

/** ⌘↵ generates and installs, ⇧⌘↵ only generates: the Standard folder's
 * keydown. */
export function generateKey(e: KeyboardEvent) {
  if (e.key !== "Enter" || !(e.metaKey || e.ctrlKey)) return;
  const { blocked, unbuildable, busy } = useSeeds.getState();
  if (busy) return;
  e.preventDefault();
  if (e.shiftKey) generateTyped(false, unbuildable);
  else generateTyped(true, blocked);
}
