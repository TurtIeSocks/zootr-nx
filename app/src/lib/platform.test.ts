import { expect, test, vi } from "vitest";
import { schemeUrl } from "@/lib/platform";

// Tauri's own base on Windows, where custom schemes are http hosts.
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  convertFileSrc: (path: string, scheme: string) =>
    `http://${scheme}.localhost/${encodeURIComponent(path)}`,
}));

test("a scheme url takes Tauri's base and keeps its path and query whole", () => {
  expect(schemeUrl("zootr-icon", "bow.png")).toBe(
    "http://zootr-icon.localhost/bow.png",
  );
  expect(schemeUrl("zootr-colors", "bow.png?t=2&c=20b060")).toBe(
    "http://zootr-colors.localhost/bow.png?t=2&c=20b060",
  );
});
