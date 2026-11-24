import type { PlandoCheck, PlandoItem } from "@/bindings";
import { sameEntries } from "@/lib/api";
import type { FullConfig } from "@/lib/config";

/** One plando: chest hash to the item actor it must hold. */
export type Plando = Record<string, string>;

/** A change to the config, worked out from the config it lands on. */
type Change = (c: FullConfig) => Partial<FullConfig>;

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

/** The saved plando the draft matches, if any. An empty draft matches
 * none. */
export const savedAs = (c: FullConfig) =>
  Object.keys(c.place).length === 0
    ? null
    : (Object.keys(c.plandos)
        .sort()
        .find((n) => samePins(c.plandos[n] ?? {}, c.place)) ?? null);

/** Whether the draft holds pins no saved plando keeps, so replacing it
 * loses them. */
export const unsaved = (c: FullConfig) =>
  Object.keys(c.place).length > 0 && savedAs(c) === null;

/** Each entry of `rec` but `key`. */
const without = <T>(rec: Record<string, T>, key: string): Record<string, T> =>
  Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));

/** The saved plando `name` as the draft. */
export const load =
  (name: string): Change =>
  (c) => ({ place: { ...c.plandos[name] } });

/** The draft kept as `name`, replacing a saved plando by that name. */
export const saveAs =
  (name: string): Change =>
  (c) => ({ plandos: { ...c.plandos, [name]: { ...c.place } } });

/** Drops the saved plando `name`. The draft stays. */
export const remove =
  (name: string): Change =>
  (c) => ({ plandos: without(c.plandos, name) });

/** Pins `item` to the check `hash` in the draft, replacing what it held. */
export const place =
  (hash: string, item: string): Change =>
  (c) => ({ place: { ...c.place, [hash]: item } });

/** Frees the check `hash` in the draft. */
export const unplace =
  (hash: string): Change =>
  (c) => ({ place: without(c.place, hash) });

/** An empty draft: the next seed shuffles every check. */
export const clear = (): Change => () => ({ place: {} });
