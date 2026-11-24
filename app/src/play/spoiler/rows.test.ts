import { expect, test } from "vitest";
import type { Spot } from "@/bindings";
import { oldSeed, seed } from "@/play/spoiler/fixture";
import {
  type Column,
  columns,
  hasLogic,
  heroAreas,
  needed,
  rule,
  ruleText,
  search,
  unlocks,
} from "@/play/spoiler/rows";

/** The spot at `i`, which the fixture always has. */
const at = (spots: Spot[], i: number) => spots[i] as Spot;

const titles = (c: Column) =>
  c.cards.map((card) =>
    card.kind === "check" ? card.spot.item_name : card.name,
  );

test("rounds become columns, checks before the sources they collect", () => {
  const got = columns(seed(), false);
  expect(got.map((c) => c.round)).toEqual([1, 2, 3, 4]);
  expect(got.map(titles)).toEqual([
    ["Fairy Slingshot", "Fairy Bow"],
    ["Bomb Bag", "Beat King Dodongo"],
    ["Zelda's Lullaby"],
    ["Megaton Hammer"],
  ]);
});

test("junk without a round never shows in a column", () => {
  const all = columns(seed(), false).flatMap(titles);
  expect(all).not.toContain("Green Rupee");
});

test("the Way of the Hero filter keeps only its checks and drops empty rounds", () => {
  const got = columns(seed(), true);
  expect(got.map((c) => c.round)).toEqual([1, 2, 3]);
  expect(got.map(titles)).toEqual([
    ["Fairy Slingshot"],
    ["Bomb Bag"],
    ["Zelda's Lullaby"],
  ]);
});

test("the Way of the Hero groups its checks by area", () => {
  const got = heroAreas(seed().spots);
  expect(got.map((a) => [a.name, a.spots.map((s) => s.hash)])).toEqual([
    ["Dodongo's Cavern", ["1"]],
    ["Hyrule Castle", ["2"]],
    ["Kokiri Forest", ["3"]],
  ]);
});

test("a rule links each item it names to the soonest check holding it", () => {
  const spots = seed().spots;

  const parts = rule(at(spots, 0), spots);
  expect(parts).toEqual([
    { item: "Item_C", name: "Fairy Slingshot", giver: spots[2] },
    { text: " and " },
    { item: "Item_D", name: "Megaton Hammer", giver: spots[5] },
    { text: " or " },
    { item: "Item_B", name: "Bomb Bag", giver: spots[0] },
  ]);
  expect(ruleText(parts)).toBe(
    "Fairy Slingshot and Megaton Hammer or Bomb Bag",
  );
});

test("an item Link starts with reads (start) in place of its check", () => {
  const spots = seed().spots;
  const start = [{ item: "Item_D", name: "Megaton Hammer", count: 1 }];
  expect(ruleText(rule(at(spots, 0), spots, start))).toBe(
    "Fairy Slingshot and Megaton Hammer (start) or Bomb Bag",
  );
});

test("an item no check holds keeps a readable id", () => {
  const spots = seed().spots;
  const parts = rule(
    {
      ...at(spots, 0),
      need: "Item_I and adult",
      need_items: ["Item_I"],
    },
    spots,
  );
  expect(parts).toEqual([
    { item: "Item_I", name: "I" },
    { text: " and adult" },
  ]);
});

test("a rule without items is its text, and no rule is nothing", () => {
  const spots = seed().spots;
  expect(rule(at(spots, 1), spots)).toEqual([{ text: "always" }]);
  expect(rule(at(oldSeed().spots, 0), spots)).toEqual([]);
});

test("search matches areas, places and items", () => {
  const spots = seed().spots;
  const hashes = (q: string) => search(spots, q).map((s) => s.hash);
  expect(hashes("lost")).toEqual(["5", "6"]);
  expect(hashes("mido")).toEqual(["3"]);
  expect(hashes("BOMB")).toEqual(["1"]);
});

test("a seed saved before the logic fields has no logic", () => {
  expect(hasLogic(seed())).toBe(true);
  expect(hasLogic(oldSeed())).toBe(false);
});

test("a spot needs the checks giving its rule's items, never itself", () => {
  const spots = seed().spots;
  // Slingshot and (Hammer or Bomb Bag): the slingshot is needed, the
  // hammer one option, and the Bomb Bag comes from the spot itself.
  expect(needed(at(spots, 0), spots)).toEqual(
    new Map([
      ["3", "all"],
      ["6", "any"],
    ]),
  );
});

test("a spot needs an item Link starts with by its id", () => {
  const spots = seed().spots;
  const start = [{ item: "Item_D", name: "Megaton Hammer", count: 1 }];
  expect(needed(at(spots, 0), spots, start)).toEqual(
    new Map([
      ["3", "all"],
      ["Item_D", "any"],
    ]),
  );
});

test("a spot needs nothing for an item no check holds or a plain rule", () => {
  const spots = seed().spots;
  const ungiven = {
    ...at(spots, 0),
    need: "Item_I and adult",
    need_items: ["Item_I"],
  };
  expect(needed(ungiven, spots).size).toBe(0);
  expect(needed(at(spots, 5), spots).size).toBe(0);
});

/** The fixture's first spot with rule `need` over the items it names. */
const ruled = (need: string, need_items: string[]) => ({
  ...at(seed().spots, 0),
  need,
  need_items,
});

test("brackets, and, or and words that aren't items decide needed or one option", () => {
  const spots = seed().spots;
  const kinds = (need: string, items: string[]) =>
    Object.fromEntries(needed(ruled(need, items), spots));
  const slingshot = "Item_C";
  const bow = "Item_F";
  const hammer = "Item_D";
  expect(
    kinds(`(${slingshot} or ${bow}) and ${hammer}`, [slingshot, bow, hammer]),
  ).toEqual({ "3": "any", "4": "any", "6": "all" });
  expect(
    kinds(`child and (${slingshot} or (adult and ${hammer}))`, [
      slingshot,
      hammer,
    ]),
  ).toEqual({ "3": "any", "6": "any" });
  // Nothing gives the gauntlets, so the slingshot is the only way.
  expect(kinds(`${slingshot} or Item_I`, [slingshot, "Item_I"])).toEqual({
    "3": "all",
  });
});

test("a rule that can't pass or won't parse needs everything it names", () => {
  const spots = seed().spots;
  const items = ["Item_C", "Item_D", "Item_I"];
  const kinds = (need: string) =>
    Object.fromEntries(needed(ruled(need, items), spots));
  // Nothing gives the gauntlets, so this rule never passes.
  expect(kinds("(Item_C or Item_D) and Item_I")).toEqual({
    "3": "all",
    "6": "all",
  });
  expect(kinds("(Item_C or Item_D")).toEqual({
    "3": "all",
    "6": "all",
  });
});

test("a check unlocks the checks whose rules need it", () => {
  const spoiler = seed();
  const { spots } = spoiler;
  // The slingshot (3) and the hammer (6) open the Bomb Bag's check (1);
  // the hammer also the stump (5).
  expect(unlocks(at(spots, 2), spots, [])).toEqual(new Set(["1"]));
  expect(unlocks(at(spots, 5), spots, [])).toEqual(new Set(["1", "5"]));
  expect(unlocks(at(spots, 3), spots, []).size).toBe(0);
});
