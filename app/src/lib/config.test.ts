import { expect, test } from "vitest";
import { blocked, full, NO_TEST, testSummary } from "@/lib/config";

test("installing waits for a dump, a target and a closed game", () => {
  const config = full({ targets: [{ name: "PC", app: "/a", mod_dir: "/m" }] });
  const target = { name: "PC", app: true, mod_dir: true, running: false };
  const status = { game: true, targets: [target] };
  expect(blocked(status, config)).toBe("");
  expect(blocked({ ...status, game: false }, config)).toBe(
    "Set the game dump in Settings first",
  );
  expect(blocked(status, full({}))).toBe("Add a target in Settings first");
  expect(
    blocked({ ...status, targets: [{ ...target, running: true }] }, config),
  ).toBe("Close the game first");
});

test("a test setup reads as where it starts and what it adds", () => {
  expect(testSummary(NO_TEST)).toBe("");
  expect(
    testSummary({
      ...NO_TEST,
      spawn: "Map/Spot",
      give: ["A", "B"],
      defaults: ["X=1"],
      progress: ["SageOfSpirit=Step02"],
    }),
  ).toBe("starts at Map/Spot · A, B · 1 flag · 1 quest step");
  expect(
    testSummary({
      ...NO_TEST,
      give: ["A", "B", "C", "D", "E"],
      defaults: ["X=1", "Y=2"],
    }),
  ).toBe("5 items · 2 flags");
});
