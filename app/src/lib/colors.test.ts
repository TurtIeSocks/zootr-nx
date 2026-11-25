import { expect, test } from "vitest";
import type { PaletteView } from "@/bindings";
import {
  modeOf,
  previewUrl,
  setAll,
  setPick,
  swatchOf,
  toggleLock,
} from "@/lib/colors";
import { full } from "@/lib/config";
import { savedSets } from "@/lib/saved-sets";

const TUNIC: PaletteView = {
  id: "kokiri-tunic",
  label: "Kokiri Tunic",
  group: "link",
  vanilla: ["#40522b"],
  names: [],
  follows: null,
  views: [],
  sheets: 7,
  problem: null,
};

test("a pick reads as its mode and its swatch", () => {
  expect(modeOf(undefined)).toBe("vanilla");
  expect(modeOf("random")).toBe("random");
  expect(modeOf({ hex: "#112233" })).toBe("colour");
  expect(swatchOf(TUNIC, "random")).toBe("#40522b");
  expect(swatchOf(TUNIC, { hex: "#112233" })).toBe("#112233");
});

test("vanilla leaves the draft, and a lock keeps a palette from the all buttons", () => {
  let c = full({});
  const apply = (change: ReturnType<typeof setPick>) => {
    c = { ...c, ...change(c) };
  };
  apply(setPick("bow", { hex: "#20b060" }));
  apply(setPick("hammer", "random"));
  expect(c.colors).toEqual({ bow: { hex: "#20b060" }, hammer: "random" });
  apply(setPick("hammer", "vanilla"));
  expect(c.colors).toEqual({ bow: { hex: "#20b060" } });
  apply(toggleLock("bow"));
  expect(c.color_locks).toEqual(["bow"]);
  apply(setAll(["bow", "hammer"], () => "random"));
  expect(c.colors).toEqual({ bow: { hex: "#20b060" }, hammer: "random" });
  apply(setAll(["bow", "hammer"], () => "vanilla"));
  expect(c.colors).toEqual({ bow: { hex: "#20b060" } });
  apply(toggleLock("bow"));
  expect(c.color_locks).toEqual([]);
});

test("a preview url carries the sheet, the colours and the view", () => {
  const url = previewUrl({
    id: "bow",
    sheet: 0,
    picked: { hex: "#20b060", split: ["#aabbcc"] },
    changes: true,
  });
  expect(url).toBe(
    "zootr-colors://localhost/bow.png?t=0&c=20b060&s=aabbcc&v=changes",
  );
  expect(previewUrl({ id: "bow", sheet: 2, picked: null })).toBe(
    "zootr-colors://localhost/bow.png?t=2",
  );
});

test("colour sets save, match and load like plandos, whatever the key order", () => {
  const sets = savedSets("colors");
  let c = full({ colors: { bow: { hex: "#20b060" }, hammer: "random" } });
  c = { ...c, ...sets.saveAs("Loud")(c) };
  expect(sets.savedAs(c)).toBe("Loud");
  c = { ...c, colors: { hammer: "random", bow: { hex: "#20b060" } } };
  expect(sets.savedAs(c)).toBe("Loud");
  c = { ...c, colors: {} };
  expect(sets.unsaved(c)).toBe(false);
  c = { ...c, ...sets.load("Loud")(c) };
  expect(c.colors).toEqual({ bow: { hex: "#20b060" }, hammer: "random" });
  c = { ...c, ...sets.remove("Loud")(c) };
  expect(sets.unsaved(c)).toBe(true);
});
