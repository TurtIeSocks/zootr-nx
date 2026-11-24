/** A placement of an actor: its actor's name and its own `Name`, if any. */
interface Placed {
  actor: string;
  label: string | null;
}

/** What tells `placed` from its actor's other placements in `list`: its own
 * name, else its place among them. None when the actor is placed once. */
export function tag<T extends Placed>(list: T[], placed: T): string | null {
  const same = list.filter((p) => p.actor === placed.actor);
  if (same.length === 1) return null;
  return placed.label ?? `#${same.indexOf(placed) + 1}`;
}

/** Whether `placed`'s actor or own name holds `query`, any case. */
export function matches(placed: Placed, query: string): boolean {
  const q = query.toLowerCase().trim();
  return [placed.actor, placed.label ?? ""].some((t) =>
    t.toLowerCase().includes(q),
  );
}
