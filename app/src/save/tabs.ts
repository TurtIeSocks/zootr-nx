import { useShallow } from "zustand/react/shallow";
import { commands, type Edit, type Pending } from "@/bindings";
import { call } from "@/lib/api";
import { useUi } from "@/stores/ui";

/** The Save page's tabs. */
export type SaveTab = "items" | "flags" | "presets";

/** What's waiting to be saved, as the commands take it. */
export const usePending = (): Pending =>
  useUi(useShallow((s) => ({ presets: s.save.presets, edits: s.save.edits })));

/** How many changes are waiting: each preset, and each thing the edits
 * change, as the backend counts them (a cell once, however many values
 * its click wrote). */
export const usePendingCount = () =>
  useUi((s) => s.save.presets.length + s.save.changes);

/** `now` with `add` on top: an element edited again keeps only its last
 * edit, which goes last. */
export const merge = (now: Edit[], add: Edit[]) => [
  ...now.filter(
    (e) => !add.some((a) => a.hash === e.hash && a.index === e.index),
  ),
  ...add,
];

/** The stage in flight, so the next reads what it left. */
let staging: Promise<unknown> = Promise.resolve();

/** Adds the edits `ask` makes of the slot as it stands to what's pending,
 * keeping only those that change the save, so a cell clicked back to
 * where it was drops out. The slot is opened with them first: a value the
 * save won't take throws here and is never staged. Stages run one at a
 * time, each against what the last left, and one whose slot was switched
 * or discarded while it ran is dropped. */
export function stageFrom(
  ask: (slot: number, pending: Pending) => Promise<Edit[]>,
): Promise<void> {
  const run = staging.then(async () => {
    const { slot, presets, edits } = useUi.getState().save;
    if (slot === null) return;
    const add = await ask(slot, { presets, edits });
    if (add.length === 0) return;
    const staged = await call(
      commands.saveStage(slot, { presets, edits: merge(edits, add) }),
    );
    const now = useUi.getState().save;
    if (now.slot !== slot || now.edits !== edits) return;
    useUi.setPath("save.edits", staged.edits);
    useUi.setPath("save.changes", staged.count);
  });
  staging = run.catch(() => {});
  return run;
}

/** Stages `add`, as {@link stageFrom} does. */
export const stage = (add: Edit[]) => stageFrom(async () => add);

/** Turns a preset on or off. Its edits apply before the single ones. */
export const togglePreset = (name: string, on: boolean) =>
  useUi.setPath("save.presets", (p) =>
    on ? [...p, name] : p.filter((n) => n !== name),
  );

/** Drops everything pending. */
export function discard() {
  useUi.setPath("save.edits", []);
  useUi.setPath("save.changes", 0);
  useUi.setPath("save.presets", []);
}

/** Picks a slot as written at `stamp`, dropping anything pending. */
export function pickSlot(slot: number, stamp: number) {
  discard();
  useUi.setPath("save.slot", slot);
  useUi.setPath("save.stamp", stamp);
}
