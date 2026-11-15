import { expect, test } from "vitest";
import { applyTheme, isDark } from "@/lib/theme";

test("system follows the system, light and dark don't", () => {
  expect(isDark("system", true)).toBe(true);
  expect(isDark("system", false)).toBe(false);
  expect(isDark("light", true)).toBe(false);
  expect(isDark("dark", false)).toBe(true);
});

test("dark sets the class on the page", () => {
  applyTheme("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  applyTheme("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});
