import { expect, test } from "vitest";
import { between, pressed } from "@/components/log-view/use-selection";
import type { Item } from "@/live/rows";

const items: Item[] = [1, 2, 3, 4].map((id) => ({
  id,
  note: `n${id}`,
  tone: "start",
}));
const plain = { shift: false, toggle: false };
const ids = (p: { ids: Set<number> }) => [...p.ids].sort();

test("between spans either way round, or just the end when the start is gone", () => {
  expect([...between(items, 3, 1)].sort()).toEqual([1, 2, 3]);
  expect([...between(items, 9, 2)]).toEqual([2]);
});

test("a press picks alone, shift spans, and the toggle adds or drops", () => {
  let p = pressed({ ids: new Set() }, 2, items, plain);
  expect(ids(p)).toEqual([2]);
  p = pressed(p, 4, items, { ...plain, shift: true });
  expect(ids(p)).toEqual([2, 3, 4]);
  p = pressed(p, 3, items, { ...plain, toggle: true });
  expect(ids(p)).toEqual([2, 4]);
  expect(p.anchor).toBe(3);
  p = pressed(p, 1, items, plain);
  expect(ids(p)).toEqual([1]);
  expect(ids(pressed(p, 1, items, plain))).toEqual([]);
});
