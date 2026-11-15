import { expect, test, vi } from "vitest";
import type { Placement, Section } from "@/bindings";
import { move } from "@/items/layout";

vi.mock("@/bindings", () => ({ TRACKER_COLUMNS: 3 }));

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
