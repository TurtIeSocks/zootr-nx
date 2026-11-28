import { expect, test, vi } from "vitest";
import type { Placement, Section } from "@/bindings";
import { columnsOf, fadedHex, missingColor, move } from "@/items/layout";

vi.mock("@/bindings", () => ({ TRACKER_COLUMNS: 10 }));

const at = (section: Section, column: number, shown = true): Placement => ({
  section,
  column,
  width: 2,
  shown,
});

const sections = [
  at("Inventory", 1),
  at("Songs", 1, false),
  at("Quest", 2),
  at("Upgrades", 1),
];

const names = (ps: Placement[] | null) => ps?.map((p) => p.section);

test("left and right stop at the outer columns and keep list order", () => {
  expect(move(sections, "Inventory", "left", 9)).toBeNull();
  const right = move(sections, "Inventory", "right", 9);
  expect(right?.[0]?.column).toBe(2);
  expect(names(right)).toEqual(names(sections));
  expect(move([at("Quest", 3)], "Quest", "right", 9)).toBeNull();
});

test("right opens one new column, never past a lone last section", () => {
  const two = [at("Inventory", 1), at("Quest", 1)];
  const opened = move(two, "Quest", "right", 9);
  expect(opened?.[1]?.column).toBe(2);
  expect(move(opened ?? [], "Quest", "right", 9)).toBeNull();
  // A hidden section further right doesn't count as company.
  expect(
    move([at("Quest", 2), at("Songs", 3, false)], "Quest", "right", 9),
  ).toBeNull();
});

test("every shown column draws, ten of them", () => {
  const ten = Array.from({ length: 10 }, (_, i) => ({
    ...at("Quest", i + 1),
  }));
  expect(columnsOf(ten, 10).map((c) => c.n)).toEqual(
    Array.from({ length: 10 }, (_, i) => i + 1),
  );
});

test("up and down swap within the column, past hidden sections", () => {
  expect(names(move(sections, "Upgrades", "up", 9))).toEqual([
    "Upgrades",
    "Songs",
    "Quest",
    "Inventory",
  ]);
  expect(move(sections, "Inventory", "up", 9)).toBeNull();
  expect(move(sections, "Upgrades", "down", 9)).toBeNull();
  expect(move(sections, "Quest", "up", 9)).toBeNull();
});

test("width stays between 1 and the section's slots", () => {
  expect(move(sections, "Quest", "wider", 2)).toBeNull();
  expect(move(sections, "Quest", "wider", 3)?.[2]?.width).toBe(3);
  expect(
    move([{ ...at("Quest", 1), width: 1 }], "Quest", "narrower", 3),
  ).toBeNull();
});

test("missing items follow the background at 72% until given a color", () => {
  const background = "#18181b";
  expect(missingColor({ missing: null, background })).toBe(
    "color-mix(in srgb, #18181b 72%, transparent)",
  );
  expect(missingColor({ missing: "#000000a6", background })).toBe("#000000a6");
  expect(fadedHex(background)).toBe("#18181bb8");
  expect(fadedHex("rebeccapurple")).toBe("rebeccapurple");
});
