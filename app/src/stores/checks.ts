import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  type ChecksEvent,
  commands,
  events,
  type Mark,
  type Row,
} from "@/bindings";
import { isDone, update } from "@/checks/rows";
import { call, message } from "@/lib/api";
import { once } from "@/stores/once";
import { useSeeds } from "@/stores/seeds";

/** The installed seed's checks, kept current by the backend's events. Kept
 * outside the views, so the sidebar can count them. */
interface Checks {
  /** The checks have come from the backend. */
  loaded: boolean;
  /** The installed seed's history entry, if one is installed. */
  id: number | null;
  /** Every check of the installed seed, as the backend last sent it. */
  rows: Row[];
  /** How many rows are done, kept in step with `rows`. */
  done: number;
  /** How many rows there are, kept in step with `rows`. */
  total: number;
  /** Why the last load failed. Rows already loaded stay on screen. */
  error?: string;
}

/** The checks store. Start it with {@link init}. */
export const useChecks = create<Checks>()(
  dotPath(
    (): Checks => ({ loaded: false, id: null, rows: [], done: 0, total: 0 }),
  ),
);

useChecks.subscribePath("rows", (rows) => {
  useChecks.setPath("done", rows.filter(isDone).length);
  useChecks.setPath("total", rows.length);
});

/** Ticks or unticks the check an event names. */
const apply = ({ hash, checked }: ChecksEvent) =>
  useChecks.setPath("rows", (rows) => update(rows, hash, { checked }));

/** Events that came while a load ran, held for after it. */
let early: ChecksEvent[] | undefined;

/** Loads the installed seed's checks. The old rows stay until the new ones
 * come, and only the latest load is taken. */
export async function load() {
  const held: ChecksEvent[] = [];
  early = held;
  try {
    const { id, rows } = await call(commands.checksState());
    if (early !== held) return;
    useChecks.setState((s) => ({ ...s, loaded: true, id, rows }));
    useChecks.setPath("error", undefined);
    held.forEach(apply);
  } catch (e) {
    if (early === held) useChecks.setPath("error", message(e));
  }
  if (early === held) early = undefined;
}

/** Listens for flips and loads again whenever another seed is installed.
 * Stopping undoes both. */
const started = once(async () => {
  // Listen first, then load: a flip is safe to apply twice.
  const unlisten = await events.checksEvent.listen(({ payload }) =>
    early ? early.push(payload) : apply(payload),
  );
  const unsubscribe = useSeeds.subscribePath("history.installed.id", () => {
    void load();
  });
  await load();
  return () => {
    unlisten();
    unsubscribe();
    early = undefined;
  };
});

export const { init, stop } = started;

/** Marks the check (skipped, a note, or none) on the installed seed.
 * Throws what saving failed with. */
export async function mark(hash: string, mark: Mark | null) {
  const { id } = useChecks.getState();
  if (id === null) return;
  await call(commands.checksMark(id, hash, mark));
  useChecks.setPath("rows", (rows) => update(rows, hash, { mark }));
}

/** Takes a flag's new label, set from Live or from here. Blank drops it. */
export const relabel = (flag: number, label: string) =>
  useChecks.setPath("rows", (rows) =>
    rows.map((r) => (r.flag === flag ? { ...r, label: label || null } : r)),
  );
