import { expect, test } from "vitest";
import { area, areas, mapAge } from "@/dev/areas";

const PLACES = [
  "Kakariko_Child/Kakariko_FrontOfGate",
  "Kakariko_Adult/Kakariko_FrontOfGate",
  "Hill_Child/Hill_FrontOfMarket",
  "Hill_Child/Hill_FrontOfKakarikoGate",
  "DekuTree/DekuTree_Entrance",
];

test("a map's area drops its age, and its age reads from the suffix", () => {
  expect(area("Kakariko_Child")).toBe("Kakariko");
  expect(area("DekuTree")).toBe("DekuTree");
  expect(mapAge("Kakariko_Adult")).toBe("adult");
  expect(mapAge("DekuTree")).toBeUndefined();
});

test("areas keep Link's age and maps for either age, sorted", () => {
  const child = areas(PLACES, true, "");
  expect(child.map((a) => a.area)).toEqual(["DekuTree", "Hill", "Kakariko"]);
  expect(child[2]?.spots.map((s) => s.map)).toEqual(["Kakariko_Child"]);
  const adult = areas(PLACES, false, "");
  expect(adult.map((a) => a.area)).toEqual(["DekuTree", "Kakariko"]);
});

test("with no age known, both show", () => {
  const kak = areas(PLACES, undefined, "").find((a) => a.area === "Kakariko");
  expect(kak?.spots).toHaveLength(2);
});

test("the query matches the area or the spot, any case", () => {
  expect(areas(PLACES, true, "kakariko").flatMap((a) => a.spots)).toHaveLength(
    2,
  );
  expect(areas(PLACES, true, "market")[0]?.spots[0]?.spot).toBe(
    "Hill_FrontOfMarket",
  );
});
