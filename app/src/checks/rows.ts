import type { Row } from "@/bindings";

/** An area's shown rows and its whole count. */
export interface Area {
  name: string;
  /** The rows shown, after the filter. */
  rows: Row[];
  done: number;
  total: number;
}

/** What a check's text dialog edits. */
export type Edit = "Note" | "Name";

/** The user's label, else the place, else what it held. History fills the
 * names in for every seed; the ids only satisfy types. */
export const nameOf = (r: Row) =>
  r.label || r.spot.place || r.spot.chest_name || r.spot.vanilla;
/** The row's area by name. */
export const areaOf = (r: Row) => r.spot.area_name || r.spot.area;

/** Checked, or set aside. */
export const isDone = (r: Row) => r.checked || r.mark?.kind === "Skipped";

/** Orders names alphabetically. */
const byName = (a: string, b: string) => a.localeCompare(b);

/** Which rows show, in the picker's order: every one, those not done, or
 * those with a mark. */
export const FILTERS = [
  { id: "all", label: "All", keep: () => true },
  { id: "remaining", label: "Remaining", keep: (r: Row) => !isDone(r) },
  { id: "marked", label: "Marked", keep: (r: Row) => r.mark !== null },
] as const;

/** A filter's id. */
export type Filter = (typeof FILTERS)[number]["id"];

/** The rows `filter` keeps. */
const keeps = (filter: Filter) =>
  FILTERS.find((f) => f.id === filter)?.keep ?? (() => true);

/** Rows by area, both sorted by name. Counts cover the whole area, the
 * search and `filter` only change what's shown. */
export function areas(rows: Row[], query: string, filter: Filter): Area[] {
  const q = query.toLowerCase();
  const keep = keeps(filter);
  const groups = new Map<string, Row[]>();
  for (const r of rows)
    groups.set(areaOf(r), [...(groups.get(areaOf(r)) ?? []), r]);
  return [...groups]
    .sort(([a], [b]) => byName(a, b))
    .map(([name, all]) => ({
      name,
      done: all.filter(isDone).length,
      total: all.length,
      rows: all
        .filter(keep)
        .filter((r) =>
          [name, nameOf(r)].some((f) => f.toLowerCase().includes(q)),
        )
        .sort((a, b) => byName(nameOf(a), nameOf(b))),
    }))
    .filter((a) => a.rows.length > 0);
}

/** `rows` with one spot's row changed. */
export const update = (rows: Row[], hash: string, patch: Partial<Row>) =>
  rows.map((r) => (r.spot.hash === hash ? { ...r, ...patch } : r));
