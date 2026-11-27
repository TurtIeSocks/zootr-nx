import type { Spoiler, Spot } from "@/bindings";

/** A spot for tests, holding `item` with the logic `patch` gives. */
export const spot = (
  hash: string,
  area: string,
  place: string,
  item: [id: string, name: string],
  patch: Partial<Spot> = {},
): Spot => ({
  hash,
  area,
  holder: "Obj_A",
  vanilla: "Item_A",
  item: item[0],
  area_name: area,
  chest_name: place,
  item_name: item[1],
  place,
  ...patch,
});

/** A seed with logic: four rounds, a source, a junk check and the Way of
 * the Hero through the slingshot and the bomb bag. */
export const seed = (): Spoiler => ({
  spots: [
    spot("1", "Dodongo's Cavern", "Map", ["Item_B", "Bomb Bag"], {
      round: 2,
      hero: true,
      need: "Item_C and Item_D|Item_B",
      need_items: ["Item_B", "Item_D", "Item_C"],
    }),
    spot("2", "Hyrule Castle", "Courtyard", ["Item_E", "Zelda's Lullaby"], {
      round: 3,
      hero: true,
      need: "always",
    }),
    spot("3", "Kokiri Forest", "Mido's House", ["Item_C", "Fairy Slingshot"], {
      round: 1,
      hero: true,
      need: "always",
    }),
    spot("4", "Kokiri Forest", "Grotto", ["Item_F", "Fairy Bow"], {
      round: 1,
      need: "always",
    }),
    spot("5", "Lost Woods", "Stump", ["Item_G", "Green Rupee"], {
      need: "Item_D",
      need_items: ["Item_D"],
    }),
    spot("6", "Lost Woods", "Bridge", ["Item_D", "Megaton Hammer"], {
      round: 4,
      need: "adult",
    }),
  ],
  events: [{ round: 2, name: "Beat King Dodongo" }],
  unprovable: ["Spirit Temple: Upper Chest"],
  start: {
    items: [{ item: "Item_H", name: "Fairy Ocarina", count: 1 }],
    settings: ["Open Forest"],
  },
  entrances: [],
  key_rings: [],
});

/** The same seed as saved before spots kept their logic. */
export const oldSeed = (): Spoiler => ({
  spots: seed().spots.map(
    ({ round: _round, hero: _hero, need: _need, need_items: _items, ...s }) =>
      s,
  ),
  events: [],
  unprovable: [],
  start: { items: [], settings: [] },
  entrances: [],
  key_rings: [],
});
