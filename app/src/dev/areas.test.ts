import { expect, test } from "vitest";
import { area, areas, mapAge } from "@/dev/areas";

const PLACES = [
  "FakeKakariko_Child/FakeKakariko_FrontOfGate",
  "FakeKakariko_Adult/FakeKakariko_FrontOfGate",
  "FakeHill_Child/FakeHill_FrontOfMarket",
  "FakeHill_Child/FakeHill_FrontOfKakarikoGate",
  "DekuTree/FakeDekuTree_Entrance",
];

test("a map's area drops its age, and its age reads from the suffix", () => {
  expect(area("FakeKakariko_Child")).toBe("FakeKakariko");
  expect(area("DekuTree")).toBe("DekuTree");
  expect(mapAge("FakeKakariko_Adult")).toBe("adult");
  expect(mapAge("DekuTree")).toBeUndefined();
});

test("areas keep Link's age and maps for either age, sorted", () => {
  const child = areas(PLACES, true, "");
  expect(child.map((a) => a.area)).toEqual([
    "DekuTree",
    "FakeHill",
    "FakeKakariko",
  ]);
  expect(child[2]?.spots.map((s) => s.map)).toEqual(["FakeKakariko_Child"]);
  const adult = areas(PLACES, false, "");
  expect(adult.map((a) => a.area)).toEqual(["DekuTree", "FakeKakariko"]);
});

test("Link's area comes first, then the rest sorted", () => {
  const here = areas(PLACES, true, "", "FakeKakariko_Child");
  expect(here.map((a) => a.area)).toEqual([
    "FakeKakariko",
    "DekuTree",
    "FakeHill",
  ]);
});

test("with no age known, both show", () => {
  const kak = areas(PLACES, undefined, "").find(
    (a) => a.area === "FakeKakariko",
  );
  expect(kak?.spots).toHaveLength(2);
});

test("the query matches the area or the spot, any case", () => {
  expect(areas(PLACES, true, "kakariko").flatMap((a) => a.spots)).toHaveLength(
    2,
  );
  expect(areas(PLACES, true, "market")[0]?.spots[0]?.spot).toBe(
    "FakeHill_FrontOfMarket",
  );
});
