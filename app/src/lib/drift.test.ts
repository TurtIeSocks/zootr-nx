import { expect, test } from "vitest";
import { full } from "@/lib/config";
import {
  changesAnything,
  NO_PANES,
  type PaneBase,
  paneChanges,
  resetPane,
} from "@/lib/drift";

const base: PaneBase = {
  colors: { hair: "random", bow: { hex: "#112233" } },
  audio: { pools: { area: "shuffle" }, tracks: { market: "vanilla" } },
  place: { 7: "Item_A" },
};

test("a palette, a pool, a track and a pin each count once", () => {
  const config = full({
    colors: { hair: "random", bow: { hex: "#445566" } },
    audio: { pools: { area: "random" } },
    place: { 7: "Item_A", 8: "Item_B" },
  });
  expect(paneChanges(base, config)).toEqual([
    {
      kind: "palette",
      key: "bow",
      before: { hex: "#112233" },
      after: { hex: "#445566" },
    },
    { kind: "pool", key: "area", before: "shuffle", after: "random" },
    { kind: "track", key: "market", before: "vanilla", after: undefined },
    { kind: "pin", key: "8", before: undefined, after: "Item_B" },
  ]);
});

test("vanilla palettes and pools are unset, as the backend reads them", () => {
  const config = full({
    colors: { hair: "vanilla" },
    audio: { pools: { area: "vanilla" } },
  });
  expect(paneChanges(NO_PANES, config)).toEqual([]);
});

test("each pane resets to the base, a locked palette keeping its pick", () => {
  const config = full({
    colors: { bow: "random", hearts: { hex: "#ff0000" } },
    color_locks: ["bow", "hair"],
    audio: { pools: { battle: "random" } },
    place: { 9: "Item_C" },
  });
  const apply = (pane: "colors" | "audio" | "plando", to: PaneBase) =>
    resetPane(pane, to)(config);
  expect(apply("colors", base).colors).toEqual({ bow: "random" });
  expect(apply("colors", NO_PANES).colors).toEqual({ bow: "random" });
  expect(apply("audio", base).audio).toEqual(base.audio);
  expect(apply("audio", NO_PANES).audio).toEqual({});
  expect(apply("plando", base).place).toEqual(base.place);
  expect(apply("plando", NO_PANES).place).toEqual({});
});

test("a reset that lands where the config is changes nothing", () => {
  const config = full({ ...base, color_locks: [] });
  expect(changesAnything(resetPane("colors", base), config)).toBe(false);
  expect(changesAnything(resetPane("plando", base), config)).toBe(false);
  expect(changesAnything(resetPane("plando", NO_PANES), config)).toBe(true);
});
