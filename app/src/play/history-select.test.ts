import { expect, test } from "vitest";
import { NONE, toggled } from "@/play/history-select";

const shown = [5, 4, 3, 2, 1];
const ids = (sel: { ids: ReadonlySet<number> }) => [...sel.ids].sort();

test("a click flips one seed and a shift-click sets the run since the last", () => {
  const one = toggled(NONE, 4, shown, false);
  expect(ids(one)).toEqual([4]);
  const run = toggled(one, 2, shown, true);
  expect(ids(run)).toEqual([2, 3, 4]);
  // A shift-click on a picked seed clears the run back to the last click.
  const back = toggled(run, 3, shown, true);
  expect(ids(back)).toEqual([4]);
  expect(ids(toggled(back, 4, shown, false))).toEqual([]);
});

test("a shift-click with no last click, or one filtered out, flips one seed", () => {
  expect(ids(toggled(NONE, 3, shown, true))).toEqual([3]);
  const hidden = toggled(NONE, 9, [9], false);
  expect(ids(toggled(hidden, 2, shown, true))).toEqual([2, 9]);
});
