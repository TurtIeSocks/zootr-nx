import { expect, test } from "vitest";
import { type FullConfig, full } from "@/lib/config";
import {
  checkLabels,
  itemLabels,
  load,
  place,
  remove,
  saveAs,
  savedAs,
  unplace,
  unsaved,
} from "@/lib/plando";

const check = (hash: string, area: string, place: string) => ({
  hash,
  area,
  place,
  vanilla: "Green Rupee",
  needs: null,
});

test("labels repeat with a number so each picks one row", () => {
  const checks = checkLabels([
    check("1", "Kokiri Forest", "Mido's House"),
    check("2", "Kokiri Forest", "Mido's House"),
    check("3", "Lost Woods", "Target"),
  ]);
  expect([...checks.keys()]).toEqual([
    "Kokiri Forest: Mido's House",
    "Kokiri Forest: Mido's House (2)",
    "Lost Woods: Target",
  ]);
  expect(checks.get("Kokiri Forest: Mido's House (2)")?.hash).toBe("2");
  const items = itemLabels([{ actor: "Item_A", name: "Hookshot" }]);
  expect(items.get("Hookshot")?.actor).toBe("Item_A");
});

test("the draft matches a saved plando whatever its key order", () => {
  const c = full({
    place: { 2: "Item_B", 1: "Item_A" },
    plandos: { B: { 1: "Item_A", 2: "Item_B" }, A: {} },
  });
  expect(savedAs(c)).toBe("B");
  expect(unsaved(c)).toBe(false);
  const edited = { ...c, place: { 1: "Item_A" } };
  expect(savedAs(edited)).toBeNull();
  expect(unsaved(edited)).toBe(true);
  const empty = { ...c, place: {} };
  expect(savedAs(empty)).toBeNull();
  expect(unsaved(empty)).toBe(false);
});

test("pins land in the draft, then save, load and drop", () => {
  let c = full({});
  const apply = (change: (c: FullConfig) => Partial<FullConfig>) => {
    c = { ...c, ...change(c) };
  };
  apply(place("7", "Item_A"));
  apply(place("8", "Item_B"));
  apply(unplace("8"));
  expect(c.place).toEqual({ 7: "Item_A" });
  apply(saveAs("A"));
  expect(c.plandos).toEqual({ A: { 7: "Item_A" } });
  apply(place("7", "Item_C"));
  expect(c.plandos.A).toEqual({ 7: "Item_A" });
  apply(load("A"));
  expect(c.place).toEqual({ 7: "Item_A" });
  apply(remove("A"));
  expect(c.plandos).toEqual({});
  expect(c.place).toEqual({ 7: "Item_A" });
});
