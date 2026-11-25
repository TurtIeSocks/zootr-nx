import { expect, test } from "vitest";
import { distance, sortRows } from "@/dev/sort";

const row = (name: string, tag: string | null, d?: number) => ({
  name,
  tag,
  distance: d,
});
const names = (rows: ReturnType<typeof row>[]) =>
  rows.map((r) => (r.tag ? `${r.name} ${r.tag}` : r.name));

test("A–Z sorts by name, then tag in number order", () => {
  const rows = [row("Obj_B", null), row("Obj_A", "#10"), row("Obj_A", "#2")];
  expect(names(sortRows(rows, "az", (r) => r))).toEqual([
    "Obj_A #2",
    "Obj_A #10",
    "Obj_B",
  ]);
});

test("Nearest puts the nearest first, unknown distances last by name, and falls back to A–Z", () => {
  const rows = [row("Far", null, 9), row("Zed", null), row("Near", null, 1)];
  expect(names(sortRows(rows, "near", (r) => r))).toEqual([
    "Near",
    "Far",
    "Zed",
  ]);
  const unknown = rows.map((r) => ({ ...r, distance: undefined }));
  expect(names(sortRows(unknown, "near", (r) => r))).toEqual([
    "Far",
    "Near",
    "Zed",
  ]);
});

test("a distance needs both positions whole", () => {
  expect(distance([0, 0, 0], [3, 4, 0])).toBe(5);
  expect(distance(undefined, [1, 0, 0])).toBeUndefined();
  expect(distance([0, null, 0], [1, 0, 0])).toBeUndefined();
});
