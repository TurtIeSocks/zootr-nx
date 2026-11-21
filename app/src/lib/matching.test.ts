import { expect, test } from "vitest";
import { hasAll, matching, words } from "@/lib/matching";

test("matching needs a query and stops at the limit", () => {
  const names = ["FakeFlag_IsChild", "FakeFlag_CurrentMap", "Other"];
  expect(matching(names, "")).toEqual([]);
  expect(matching(names, "flag", 1)).toEqual(["FakeFlag_IsChild"]);
});

test("every word must match, in any order and any case", () => {
  const names = ["Mod folder", "Save folder", "Mod name"];
  expect(matching(names, "FOLDER mod")).toEqual(["Mod folder"]);
  expect(matching(names, "  folder  ")).toEqual(["Mod folder", "Save folder"]);
});

test("words splits on any space and lowers case; hasAll wants each", () => {
  expect(words("  Mod\tFolder ")).toEqual(["mod", "folder"]);
  expect(hasAll("App › Targets › Mod folder", ["targets", "mod"])).toBe(true);
  expect(hasAll("Mod folder", ["mod", "save"])).toBe(false);
});
