import { expect, test } from "vitest";
import { row } from "@/checks/fixture";
import { areas, isDone, nameOf, update } from "@/checks/rows";

const rows = [
  row("1", "Kokiri Forest", "Blue Rupee chest", { checked: true }),
  row("2", "Deku Tree", "Map chest"),
  row("3", "Kokiri Forest", "Green Rupee chest", { mark: { kind: "Skipped" } }),
  row("4", "Kokiri Forest", "Kokiri Sword chest"),
];

test("groups by area with counts over the whole area", () => {
  const got = areas(rows, "", "all");
  expect(got.map((a) => [a.name, a.done, a.total])).toEqual([
    ["Deku Tree", 0, 1],
    ["Kokiri Forest", 2, 3],
  ]);
  expect(got[1]?.rows.map((r) => r.spot.hash)).toEqual(["1", "3", "4"]);
});

test("searching and filtering keep the counts", () => {
  const forest = areas(rows, "", "remaining");
  expect(forest[1]?.rows.map((r) => r.spot.hash)).toEqual(["4"]);
  expect(forest[1]?.done).toBe(2);
  expect(areas(rows, "sword", "all").map((a) => a.name)).toEqual([
    "Kokiri Forest",
  ]);
  expect(areas(rows, "deku", "all")[0]?.rows).toHaveLength(1);
});

test("the marked filter shows only rows with a mark", () => {
  const got = areas(rows, "", "marked");
  expect(got.map((a) => a.name)).toEqual(["Kokiri Forest"]);
  expect(got[0]?.rows.map((r) => r.spot.hash)).toEqual(["3"]);
});

test("a skipped check counts as done", () => {
  expect(rows.map(isDone)).toEqual([true, false, true, false]);
});

test("update changes only the spot's row", () => {
  const got = update(rows, "2", { checked: true });
  expect(got[1]?.checked).toBe(true);
  expect(got[0]).toBe(rows[0]);
});

test("a label wins over the place, and the place over what it held", () => {
  const placed = row("5", "Kokiri Forest", "Blue Rupee");
  expect(nameOf(placed)).toBe("Blue Rupee");
  placed.spot.place = "Mido's House Back Left";
  expect(nameOf(placed)).toBe("Mido's House Back Left");
  placed.label = "by the bed";
  expect(nameOf(placed)).toBe("by the bed");
});
