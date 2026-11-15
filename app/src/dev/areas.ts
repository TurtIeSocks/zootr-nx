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

/** The spots for Link's age, both ages when it's unknown, grouped by area
 * in order. The query matches the area or the spot, any case. */
export function areas(
  places: string[],
  child: boolean | undefined,
  query: string,
): Area[] {
  const want = child === undefined ? undefined : child ? "child" : "adult";
  const q = query.trim().toLowerCase();
  const byArea = new Map<string, Spot[]>();
  for (const place of places) {
    const [map, spot] = place.split("/");
    if (!map || !spot) continue;
    const age = mapAge(map);
    if (want && age && age !== want) continue;
    const name = area(map);
    if (q && !`${name}/${spot}`.toLowerCase().includes(q)) continue;
    byArea.set(name, [...(byArea.get(name) ?? []), { place, map, spot }]);
  }
  return [...byArea]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([area, spots]) => ({ area, spots }));
}
