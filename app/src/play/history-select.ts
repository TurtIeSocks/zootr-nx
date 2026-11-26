/** The seeds picked in History's select mode, and the last one clicked,
 * where a shift-click's range starts. */
export interface Selection {
  ids: ReadonlySet<number>;
  last?: number;
}

export const NONE: Selection = { ids: new Set() };

/** `sel` with `id` flipped. With `range`, every seed in `shown` from the
 * last click to `id` takes the state `id` takes. */
export function toggled(
  sel: Selection,
  id: number,
  shown: readonly number[],
  range: boolean,
): Selection {
  const on = !sel.ids.has(id);
  const from = range && sel.last !== undefined ? shown.indexOf(sel.last) : -1;
  const to = shown.indexOf(id);
  const run =
    from < 0 || to < 0
      ? [id]
      : shown.slice(Math.min(from, to), Math.max(from, to) + 1);
  const ids = new Set(sel.ids);
  for (const r of run) {
    if (on) ids.add(r);
    else ids.delete(r);
  }
  return { ids, last: id };
}
