import { useShallow } from "zustand/react/shallow";
import { commands, type Edit, type Pending } from "@/bindings";
import { call } from "@/lib/api";
import { useUi } from "@/stores/ui";

/** The Save page's tabs. */
export type SaveTab = "items" | "flags" | "presets";

/** What's waiting to be saved, as the commands take it. */
export const usePending = (): Pending =>
  useUi(useShallow((s) => ({ presets: s.save.presets, edits: s.save.edits })));

/** How many changes are waiting. */
export const pendingCount = ({ presets, edits }: Pending) =>
  presets.length + edits.length;

/** `now` with `add` on top: an element edited again keeps only its last
 * edit, which goes last. */
export const merge = (now: Edit[], add: Edit[]) => [
  ...now.filter(
    (e) => !add.some((a) => a.hash === e.hash && a.index === e.index),
  ),
  ...add,
];

/** Adds edits to what's pending. The slot is opened with them first, so a
 * value the save won't take throws here and is never staged. */
export async function stage(add: Edit[]) {
  const { slot, presets, edits } = useUi.getState().save;
  if (slot === null || add.length === 0) return;
  const next = merge(edits, add);
  await call(commands.saveItems(slot, { presets, edits: next }));
  useUi.setPath("save.edits", next);
}

/** Turns a preset on or off. Its edits apply before the single ones. */
export const togglePreset = (name: string, on: boolean) =>
  useUi.setPath("save.presets", (p) =>
    on ? [...p, name] : p.filter((n) => n !== name),
  );

/** Drops everything pending. */
export function discard() {
  useUi.setPath("save.edits", []);
  useUi.setPath("save.presets", []);
}

/** Picks a slot as written at `stamp`, dropping anything pending. */
export function pickSlot(slot: number, stamp: number) {
  discard();
  useUi.setPath("save.slot", slot);
  useUi.setPath("save.stamp", stamp);
}
