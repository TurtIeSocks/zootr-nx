import { expect, test } from "vitest";
import { isLight, luminance } from "@/items/fallback-icons";

test("luminance follows WCAG, and only reads #rrggbb", () => {
  expect(luminance("#000000")).toBe(0);
  expect(luminance("#ffffff")).toBeCloseTo(1);
  expect(luminance("#00ff00")).toBeCloseTo(0.7152);
  expect(luminance("white")).toBeNull();
  expect(luminance("#fff")).toBeNull();
});

test("a background gets the ink that contrasts with it more", () => {
  expect(isLight("#ffffff")).toBe(true);
  expect(isLight("#00ff00")).toBe(true);
  // Mid grey sits just past the crossover, so black wins.
  expect(isLight("#777777")).toBe(true);
  expect(isLight("#666666")).toBe(false);
  expect(isLight("#18181b")).toBe(false);
  expect(isLight("#0000ff")).toBe(false);
  expect(isLight("white")).toBe(false);
});
