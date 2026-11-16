import { expect, test } from "vitest";
import { ago, count, seedOrCode, splitLines } from "@/lib/format";

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

test("a time reads as how long ago it was", () => {
  const now = 10 * 86_400_000;
  expect(ago(now - 20_000, now)).toBe("just now");
  expect(ago(now - 3 * 60_000, now)).toBe("3m ago");
  expect(ago(now - 2 * 3_600_000, now)).toBe("2h ago");
  expect(ago(now - 4 * 86_400_000, now)).toBe("4d ago");
});
