/** An arrival point, `MAP/SPOT`, split for showing. */
export interface Spot {
  place: string;
  map: string;
  spot: string;
}

/** A map's spots, under the map's name without its age. */
export interface Area {
  area: string;
  spots: Spot[];
}

/** A map name's age suffix. */
const AGED = /_(Child|Adult)$/;

/** A map's name without its `_Child` or `_Adult`. */
export function area(map: string) {
  return map.replace(AGED, "");
}

/** Whether a map is the child one, the adult one, or either age's. */
export function mapAge(map: string): "child" | "adult" | undefined {
  const age = AGED.exec(map)?.[1];
  if (!age) return undefined;
  return age === "Child" ? "child" : "adult";
}

/** Link's age from the game's child flag, unknown while it isn't read. */
export function ageOf(
  child: boolean | undefined,
): "child" | "adult" | undefined {
  return child === undefined ? undefined : child ? "child" : "adult";
}

/** The spots of `places` for Link's age, both ages when it's unknown, in
 * their order. The query matches the area or the spot, any case. */
export function spots(
  places: string[],
  child: boolean | undefined,
  query: string,
): Spot[] {
  const want = ageOf(child);
  const q = query.trim().toLowerCase();
  return places.flatMap((place) => {
    const [map, spot] = place.split("/");
    if (!map || !spot) return [];
    const age = mapAge(map);
    if (want && age && age !== want) return [];
    if (q && !`${area(map)}/${spot}`.toLowerCase().includes(q)) return [];
    return [{ place, map, spot }];
  });
}

/** `spots`, grouped by area, the areas sorted, the area of the map Link
 * is in (`here`) first. */
export function areas(
  places: string[],
  child: boolean | undefined,
  query: string,
  here?: string,
): Area[] {
  const byArea = new Map<string, Spot[]>();
  for (const s of spots(places, child, query)) {
    const name = area(s.map);
    byArea.set(name, [...(byArea.get(name) ?? []), s]);
  }
  const first = here && area(here);
  return [...byArea]
    .sort(
      ([a], [b]) =>
        Number(b === first) - Number(a === first) || a.localeCompare(b),
    )
    .map(([area, spots]) => ({ area, spots }));
}
