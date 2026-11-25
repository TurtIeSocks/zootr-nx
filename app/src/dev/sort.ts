import { useEffect, useState } from "react";
import { commands } from "@/bindings";
import { call } from "@/lib/api";
import { useDev } from "@/stores/dev";
import type { ActorSort } from "@/stores/ui";

/** A position as the game sends it: a float it couldn't read is null. */
export type Pos = (number | null)[];

/** How far apart `a` and `b` are, or undefined when either isn't known. */
export function distance(a?: Pos, b?: Pos): number | undefined {
  if (!a || !b || a.length !== 3 || b.length !== 3) return undefined;
  if ([...a, ...b].some((n) => n === null)) return undefined;
  return Math.hypot(...a.map((n, i) => (n ?? 0) - (b[i] ?? 0)));
}

/** What a list sorts a row by. */
export interface SortKey {
  name: string;
  tag: string | null;
  distance?: number;
}

const byName = (a: SortKey, b: SortKey) =>
  a.name.localeCompare(b.name) ||
  (a.tag ?? "").localeCompare(b.tag ?? "", undefined, { numeric: true });

/** `rows` by name then tag, or nearest first with the rows of unknown
 * distance after, by name. With no distance known, nearest is by name. */
export function sortRows<T>(
  rows: T[],
  by: ActorSort,
  key: (row: T) => SortKey,
): T[] {
  return rows
    .map((row) => ({ row, k: key(row) }))
    .sort(({ k: a }, { k: b }) => {
      if (by === "near") {
        const far = (k: SortKey) => k.distance ?? Number.POSITIVE_INFINITY;
        const d = far(a) - far(b);
        if (d) return d;
      }
      return byName(a, b);
    })
    .map(({ row }) => row);
}

/** How often Link's position is read while a list sorts by it. */
export const LINK_EVERY_MS = 2000;

/** Where Link stands, read while `on`: at once, on each map change, then
 * every {@link LINK_EVERY_MS}. Unknown until the first read answers, or
 * while a read fails. */
export function useLinkPos(on: boolean): Pos | undefined {
  const [map] = useDev.usePath("game.state.map");
  const [at, setAt] = useState<Pos>();

  useEffect(() => {
    setAt(undefined);
    if (!on || !map) return;
    let live = true;
    const read = () =>
      call(commands.devLink())
        .then((p) => live && setAt(p))
        .catch(() => live && setAt(undefined));
    void read();
    const every = setInterval(read, LINK_EVERY_MS);
    return () => {
      live = false;
      clearInterval(every);
    };
  }, [on, map]);
  return at;
}
