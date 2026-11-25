import type { PlandoCheck, PlandoItem } from "@/bindings";
import { sameEntries } from "@/lib/api";
import { type Change, savedSets, without } from "@/lib/saved-sets";

/** One plando: chest hash to the item actor it must hold. */
export type Plando = Record<string, string>;

/** `rows` by their `label`, made unique: a repeat gets `(2)`, `(3)` and
 * so on. */
function unique<T>(rows: T[], label: (row: T) => string): Map<string, T> {
  const seen = new Map<string, number>();
  const out = new Map<string, T>();
  for (const row of rows) {
    const base = label(row);
    const n = (seen.get(base) ?? 0) + 1;
    seen.set(base, n);
    out.set(n === 1 ? base : `${base} (${n})`, row);
  }
  return out;
}

/** Each check by the label its picker shows, `Area: Place`. */
export const checkLabels = (checks: PlandoCheck[]) =>
  unique(checks, (c) => `${c.area}: ${c.place}`);

/** Each item by the label its picker shows, its name. */
export const itemLabels = (items: PlandoItem[]) => unique(items, (i) => i.name);

/** Whether two plandos pin the same items, whatever order their keys came
 * in. */
export const samePins = (a: Plando, b: Plando) => sameEntries(a, b);

/** The saved plando the draft matches, whether it holds unsaved pins, and
 * the changes that load, keep and drop saved plandos. */
export const { savedAs, unsaved, load, saveAs, remove } = savedSets("place");

/** Pins `item` to the check `hash` in the draft, replacing what it held. */
export const place =
  (hash: string, item: string): Change =>
  (c) => ({ place: { ...c.place, [hash]: item } });

/** Frees the check `hash` in the draft. */
export const unplace =
  (hash: string): Change =>
  (c) => ({ place: without(c.place, hash) });
