import { expect, test } from "vitest";
import type { Choice, SettingDef } from "@/bindings";
import type { Settings } from "@/lib/config";
import {
  affects,
  chosen,
  defaults,
  forcedPick,
  forcer,
  isChanged,
  isOn,
  markedChips,
  patched,
  settingsChanges,
  startForcers,
  startGivers,
  startOf,
  summary,
  summaryParts,
  unmet,
  withForced,
} from "@/lib/settings";

/** A toggle `id`, off unless `over` says otherwise, labelled in capitals. */
const def = (id: string, over: Partial<SettingDef> = {}): SettingDef => ({
  id,
  group: "g",
  label: id.toUpperCase(),
  description: "",
  kind: { type: "toggle", default: false },
  parent: null,
  requires: null,
  section: "Fake section",
  section_about: "Fake about",
  list: false,
  gives: [],
  archipelago: { ap: "offered" },
  ...over,
});

/** A choice's option, forcing and starting nothing unless `over` says. */
const opt = (
  id: string,
  label: string,
  over: Partial<Choice> = {},
): Choice => ({
  id,
  label,
  forces: [],
  starts: [],
  archipelago: { ap: "offered" },
  ...over,
});

/** A choice `id` of `options`, the first its default. */
const choice = (
  id: string,
  options: Choice[],
  over: Partial<SettingDef> = {},
): SettingDef =>
  def(id, {
    kind: { type: "choice", default: options[0]?.id ?? "", options },
    ...over,
  });

/** `n` toggles, s0 up, all on by default. */
const toggles = (n: number): SettingDef[] =>
  Array.from({ length: n }, (_, i) =>
    def(`s${i}`, { kind: { type: "toggle", default: true } }),
  );

test("lists a few settings and counts many", () => {
  expect(summary([], {})).toBe("Nothing extra");
  expect(summary(toggles(4), {})).toBe("S0, S1, S2, S3");
  expect(summary(toggles(5), {})).toBe("5 settings on");
  expect(summary(toggles(5), { s0: false })).toBe("S1, S2, S3, S4");
});

test("a summary splits into one part per setting for badges", () => {
  expect(summaryParts([], {})).toEqual(["Nothing extra"]);
  expect(summaryParts(toggles(2), {})).toEqual(["S0", "S1"]);
  expect(summaryParts(toggles(5), {})).toEqual(["5 settings on"]);
});

test("a parent turns its children on and stands in for them", () => {
  const child = def("s1", { parent: "s0" });
  const schema = [def("s0"), child, def("s2", { parent: "s0" })];
  expect(isOn(child, { s1: false }, schema)).toBe(false);
  expect(isOn(child, { s0: true, s1: false }, schema)).toBe(true);
  expect(summary(schema, { s0: true })).toBe("S0");
  expect(summary(schema, { s2: true })).toBe("S2");
});

/** A toggle `door`, and the choice `age` whose Adult forces it on and starts
 * with a sword. */
const door = def("door", { label: "Door" });
const age = choice(
  "age",
  [
    opt("child", "Child"),
    opt("adult", "Adult", { forces: ["door"], starts: ["sword"] }),
  ],
  { label: "Age" },
);

/** The choice `gate`, open by default, and an `age` whose Adult forces it
 * open. */
const gate = choice("gate", [opt("open", "Open"), opt("shut", "Shut")], {
  label: "Gate",
});
const opens = choice(
  "age",
  [opt("child", "Child"), opt("adult", "Adult", { forces: ["gate=open"] })],
  { label: "Age" },
);

test("a choice's pick forces toggles on and gives start words", () => {
  const schema = [age, door];
  expect(forcer(door, { door: false }, schema)).toBeUndefined();
  expect(isOn(door, { door: false }, schema)).toBe(false);
  const adult = { age: "adult", door: false };
  expect(forcer(door, adult, schema)).toBe(age);
  expect(isOn(door, adult, schema)).toBe(true);
  expect(summary(schema, adult)).toBe("Age: Adult");
  expect(startForcers(schema, adult)).toEqual([{ def: age, words: ["sword"] }]);
  expect(startForcers(schema, {})).toEqual([]);
});

test("a choice's pick forces another choice's pick", () => {
  const gated = [opens, gate];
  const shut = { age: "adult", gate: "shut" };
  expect(forcer(gate, shut, gated)).toBe(opens);
  expect(forcedPick(gate, shut, gated)?.id).toBe("open");
  expect(forcedPick(gate, { gate: "shut" }, gated)).toBeUndefined();
  expect(summary(gated, shut)).toBe("Age: Adult");
});

test("whatever its pick, a choice affects what any pick forces", () => {
  const gated = [opens, gate];
  expect(affects(opens, [door, ...gated])).toEqual([gate]);
  expect(affects(age, [door, ...gated])).toEqual([door]);
  expect(affects(gate, gated)).toEqual([]);
  expect(affects(door, [age, door])).toEqual([]);
});

test("forced start words join an inventory once each", () => {
  expect(withForced({ sword: 2, bow: 1 }, ["sword", "hook"])).toEqual({
    sword: 2,
    bow: 1,
    hook: 1,
  });
});

test("a setting is left out while the toggle it needs is off", () => {
  const gifts = def("s0", { label: "S0" });
  const reward = def("s1", {
    label: "S1",
    kind: { type: "toggle", default: true },
    requires: gifts.id,
  });
  const schema = [gifts, reward];
  expect(unmet(reward, {}, schema)?.id).toBe(gifts.id);
  expect(summary(schema, {})).toBe("Nothing extra");
  expect(unmet(reward, { s0: true }, schema)).toBeUndefined();
  expect(summary(schema, { s0: true })).toBe("S0, S1");
});

test("a choice is summed up only off its default", () => {
  const size = choice("size", [opt("big", "Big"), opt("small", "Small")], {
    label: "Size",
  });
  expect(summary([size], {})).toBe("Nothing extra");
  expect(summary([size], { size: "small" })).toBe("Size: Small");
  expect(chosen(size, { size: "gone" })?.id).toBe("big");
  expect(isOn(size, { size: "small" }, [size])).toBe(false);
  expect(isChanged(size, { size: "gone" })).toBe(false);
  expect(isChanged(size, { size: "small" })).toBe(true);
});

test("a toggle is changed only when its own value leaves the default", () => {
  const on = def("s0", { kind: { type: "toggle", default: true } });
  expect(isChanged(on, {})).toBe(false);
  expect(isChanged(on, { s0: true })).toBe(false);
  expect(isChanged(on, { s0: false })).toBe(true);
});

test("a patch sets values and puts undefined ones back to default", () => {
  const settings = { s0: false, other: true };
  expect(patched(settings, { s1: true, other: undefined })).toEqual({
    s0: false,
    s1: true,
  });
  expect(patched(settings, defaults(toggles(1)))).toEqual({ other: true });
});

/** Toggles `all` (with its child `one`) and `x`, and the choice `size`. */
function diffSchema(): SettingDef[] {
  const size = choice("size", [
    opt("a", "Big"),
    opt("b", "Small"),
    opt("c", "Tiny"),
  ]);
  return [def("all"), def("one", { parent: "all" }), def("x"), size];
}

test("a diff lists effective changes, with a parent standing in", () => {
  const schema = diffSchema();
  const lines = (before: Settings, after: Settings) =>
    settingsChanges(schema, before, after).map(
      (c) => `${c.def.label}: ${c.before} → ${c.after}`,
    );
  expect(lines({}, { x: false, size: "a" })).toEqual([]);
  expect(lines({}, { x: true, size: "b" })).toEqual([
    "X: off → on",
    "SIZE: Big → Small",
  ]);
  expect(lines({ one: true }, { all: true })).toEqual(["ALL: off → on"]);
});

test("chips are marked new or gone against a base", () => {
  const schema = diffSchema();
  const marked = (settings: Settings, base: Settings | null) =>
    markedChips(schema, schema, settings, base);
  expect(marked({ x: true, size: "b" }, null)).toEqual([
    { id: "x", label: "X" },
    { id: "size", label: "SIZE", value: "Small" },
  ]);
  expect(marked({ x: true, size: "c" }, { x: true, size: "b" })).toEqual([
    { id: "x", label: "X" },
    { id: "size", label: "SIZE", value: "Tiny", mark: "new", was: "Small" },
  ]);
  expect(marked({ size: "b" }, { x: true })).toEqual([
    { id: "x", label: "X", mark: "gone" },
    { id: "size", label: "SIZE", value: "Small", mark: "new" },
  ]);
  // A child its parent now covers isn't gone: the parent stands in.
  expect(marked({ all: true }, { one: true })).toEqual([
    { id: "all", label: "ALL", mark: "new" },
  ]);
});

test("a starting inventory is changed while it differs from its default", () => {
  const start = def("s0", {
    label: "Start",
    kind: { type: "inventory", default: {} },
  });
  expect(isChanged(start, {})).toBe(false);
  expect(isChanged(start, { s0: {} })).toBe(false);
  const two = { s0: { hookshot: 1, bombs: 20 } };
  expect(isChanged(start, two)).toBe(true);
  expect(isOn(start, two, [start])).toBe(false);
  expect(summary([start], two)).toBe("Start: 2 items");
  expect(settingsChanges([start], {}, two)).toMatchObject([
    { before: "nothing", after: "hookshot, bombs ×20" },
  ]);
  // A swap that holds as many things is still new, and says what it was.
  const swapped = { s0: { bow: 1, bombs: 20 } };
  expect(markedChips([start], [start], swapped, two)).toEqual([
    { id: "s0", label: "Start", value: "2 items", mark: "new", was: "2 items" },
  ]);
  expect(markedChips([start], [start], two, two)).toEqual([
    { id: "s0", label: "Start", value: "2 items" },
  ]);
});

test("an unset starting inventory holds its default, and emptied it's changed", () => {
  const start = def("s0", {
    label: "Start",
    kind: { type: "inventory", default: { "master-sword": 1 } },
  });
  expect(startOf(start, {})).toEqual({ "master-sword": 1 });
  expect(isChanged(start, {})).toBe(false);
  expect(isChanged(start, { s0: { "master-sword": 1 } })).toBe(false);
  expect(isChanged(start, { s0: {} })).toBe(true);
  expect(summary([start], {})).toBe("Start: 1 item");
});

test("a setting's value names what it gives the grid", () => {
  const met = def("fake-met", {
    label: "Met Fake",
    gives: [["on", ["zeldas-letter"]]],
  });
  const fort = choice(
    "fake-fort",
    [opt("normal", "Normal"), opt("freed", "Freed")],
    {
      label: "Fake Fort",
      gives: [["freed", ["gerudo-card"]]],
    },
  );
  const schema = [met, fort];
  expect(startGivers(schema, {})).toEqual([]);
  const given = startGivers(schema, { "fake-met": true, "fake-fort": "freed" });
  expect(given.map((g) => [g.name, g.words])).toEqual([
    ["Met Fake", ["zeldas-letter"]],
    ["Fake Fort: Freed", ["gerudo-card"]],
  ]);
});
