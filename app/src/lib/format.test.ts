import { expect, test } from "vitest";
import { count, seedOrCode, splitLines } from "@/lib/format";

test("a seed is its digits, a share code is kept whole", () => {
  expect(seedOrCode(" 48-29 13 ")).toBe("482913");
  expect(seedOrCode(" ZR1-abc ")).toBe("ZR1-abc");
});

test("lines are trimmed and blank ones dropped", () => {
  expect(splitLines(" a=1 \n\n  \nb=2")).toEqual(["a=1", "b=2"]);
});

test("a count says one or many", () => {
  expect(count(1, "flag")).toBe("1 flag");
  expect(count(2, "flag")).toBe("2 flags");
});
