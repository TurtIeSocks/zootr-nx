import type { PaletteView, Pick, Picked } from "@/bindings";
import type { FullConfig } from "@/lib/config";
import { type Change, without } from "@/lib/saved-sets";

/** The picker's preset swatches: plain colours with names, not a mode. */
export const PRESETS: readonly [name: string, hex: string][] = [
  ["Kokiri Green", "#2f8f3a"],
  ["Goron Red", "#b0201e"],
  ["Zora Blue", "#2a55c0"],
  ["Navi White", "#e8f2ff"],
  ["Gold", "#d4a020"],
  ["Silver", "#b8bcc4"],
  ["Black", "#1a1a1a"],
  ["White", "#f4f4f4"],
  ["Purple", "#7a30b0"],
  ["Pink", "#e060a8"],
  ["Orange", "#e07018"],
  ["Teal", "#18a0a0"],
];

/** What a palette's row sets it to. */
export type Mode = "vanilla" | "random" | "colour";

/** What `pick` is, as the row's three-way switch shows it. */
export const modeOf = (pick: Pick | undefined): Mode =>
  pick === undefined || pick === "vanilla"
    ? "vanilla"
    : pick === "random"
      ? "random"
      : "colour";

/** The colour `palette`'s swatch shows: the picked one, or vanilla's. */
export const swatchOf = (palette: PaletteView, pick: Pick | undefined) =>
  typeof pick === "object" ? pick.hex : (palette.vanilla[0] ?? "#000000");

/** `picked` with slot `slot` set to `hex`: the first, or a split one. */
export const withSlot = (picked: Picked, slot: number, hex: string): Picked =>
  slot === 0
    ? { ...picked, hex }
    : {
        ...picked,
        split: (picked.split ?? []).map((s, i) => (i === slot - 1 ? hex : s)),
      };

/** The first option of a palette's switch: vanilla, or the palette it
 * moves with while left alone. */
export const vanillaLabel = (palette: PaletteView) =>
  palette.follows ? `With ${palette.follows}` : "Vanilla";

/** Sets palette `id` to `pick`. Vanilla leaves the draft, as it's what an
 * unnamed palette is. */
export const setPick =
  (id: string, pick: Pick): Change =>
  (c) => ({
    colors:
      pick === "vanilla" ? without(c.colors, id) : { ...c.colors, [id]: pick },
  });

/** Sets every unlocked palette of `ids` to what `pick` gives it. */
export const setAll =
  (ids: string[], pick: (id: string) => Pick): Change =>
  (c) => {
    const colors = { ...c.colors };
    for (const id of ids.filter((id) => !c.color_locks.includes(id))) {
      const p = pick(id);
      if (p === "vanilla") delete colors[id];
      else colors[id] = p;
    }
    return { colors };
  };

/** Locks palette `id` against Randomize all and Reset all, or unlocks it. */
export const toggleLock =
  (id: string): Change =>
  (c: FullConfig) => ({
    color_locks: c.color_locks.includes(id)
      ? c.color_locks.filter((l) => l !== id)
      : [...c.color_locks, id].sort(),
  });

/** What a preview shows of a palette. */
export interface PreviewOf {
  id: string;
  /** Which of its sheets (textures, then material spheres), from 0. */
  sheet: number;
  /** The colour painted, or null for vanilla. */
  picked: Picked | null;
  /** Grey what the palette leaves alone. */
  changes?: boolean;
}

const digits = (hex: string) => hex.replace(/^#/, "");

/** The backend's render of `preview`, as the seed would paint it. */
export function previewUrl({ id, sheet, picked, changes }: PreviewOf) {
  const query = new URLSearchParams({ t: String(sheet) });
  if (picked) {
    query.set("c", digits(picked.hex));
    for (const s of picked.split ?? []) query.append("s", digits(s));
  }
  if (changes) query.set("v", "changes");
  return `zootr-colors://localhost/${id}.png?${query}`;
}
