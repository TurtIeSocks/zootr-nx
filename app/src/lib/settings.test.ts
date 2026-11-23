import { expect, test } from "vitest";
import type { SettingDef } from "@/bindings";
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

const toggles = (n: number): SettingDef[] =>
  Array.from({ length: n }, (_, i) => ({
    id: `s${i}`,
    group: "g",
    label: `S${i}`,
    description: "",
    kind: { type: "toggle", default: true },
    parent: null,
    requires: null,
    section: "Fake section",
    section_about: "Fake about",
    list: false,
    gives: [],
    archipelago: { ap: "offered" },
  }));

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
  const off = (id: string, parent: string | null): SettingDef => ({
    id,
    group: "g",
    label: id.toUpperCase(),
    description: "",
    kind: { type: "toggle", default: false },
    parent,
    requires: null,
    section: "Fake section",
    section_about: "Fake about",
    list: false,
    gives: [],
    archipelago: { ap: "offered" },
  });
  const child = off("s1", "s0");
  const schema = [off("s0", null), child, off("s2", "s0")];
  expect(isOn(child, { s1: false }, schema)).toBe(false);
  expect(isOn(child, { s0: true, s1: false }, schema)).toBe(true);
  expect(summary(schema, { s0: true })).toBe("S0");
  expect(summary(schema, { s2: true })).toBe("S2");
});

test("a choice's pick forces toggles on and gives start words", () => {
  const [base] = toggles(1) as [SettingDef];
  const door: SettingDef = { ...base, id: "door", label: "Door" };
  const age: SettingDef = {
    ...base,
    id: "age",
    label: "Age",
    kind: {
      type: "choice",
      default: "child",
      options: [
        {
          id: "child",
          label: "Child",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "adult",
          label: "Adult",
          forces: ["door"],
          starts: ["sword"],
          archipelago: { ap: "offered" },
        },
      ],
    },
  };
  const schema = [age, door];
  expect(forcer(door, { door: false }, schema)).toBeUndefined();
  expect(isOn(door, { door: false }, schema)).toBe(false);
  const adult = { age: "adult", door: false };
  expect(forcer(door, adult, schema)).toBe(age);
  expect(isOn(door, adult, schema)).toBe(true);
  expect(summary(schema, adult)).toBe("Age: Adult");
  expect(startForcers(schema, adult)).toEqual([{ def: age, words: ["sword"] }]);
  expect(startForcers(schema, {})).toEqual([]);
  const gate: SettingDef = {
    ...base,
    id: "gate",
    label: "Gate",
    kind: {
      type: "choice",
      default: "open",
      options: [
        {
          id: "shut",
          label: "Shut",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "open",
          label: "Open",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
      ],
    },
  };
  const opens: SettingDef = {
    ...age,
    kind: {
      type: "choice",
      default: "child",
      options: [
        {
          id: "child",
          label: "Child",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "adult",
          label: "Adult",
          forces: ["gate=open"],
          starts: [],
          archipelago: { ap: "offered" },
        },
      ],
    },
  };
  const gated = [opens, gate];
  const shut = { age: "adult", gate: "shut" };
  expect(forcer(gate, shut, gated)).toBe(opens);
  expect(forcedPick(gate, shut, gated)?.id).toBe("open");
  expect(forcedPick(gate, { gate: "shut" }, gated)).toBeUndefined();
  expect(summary(gated, shut)).toBe("Age: Adult");
  // Whatever its pick, a choice affects what any pick forces.
  expect(affects(opens, [door, ...gated])).toEqual([gate]);
  expect(affects(age, [door, ...gated])).toEqual([door]);
  expect(affects(gate, gated)).toEqual([]);
  expect(affects(door, [age, door])).toEqual([]);
  expect(withForced({ sword: 2, bow: 1 }, ["sword", "hook"])).toEqual({
    sword: 2,
    bow: 1,
    hook: 1,
  });
});

test("a setting is left out while the toggle it needs is off", () => {
  const [s0, s1] = toggles(2) as [SettingDef, SettingDef];
  const gifts: SettingDef = { ...s0, kind: { type: "toggle", default: false } };
  const reward: SettingDef = { ...s1, requires: gifts.id };
  const schema = [gifts, reward];
  expect(unmet(reward, {}, schema)?.id).toBe(gifts.id);
  expect(summary(schema, {})).toBe("Nothing extra");
  expect(unmet(reward, { s0: true }, schema)).toBeUndefined();
  expect(summary(schema, { s0: true })).toBe("S0, S1");
});

test("a choice is summed up only off its default", () => {
  const size: SettingDef = {
    id: "size",
    group: "g",
    label: "Size",
    description: "",
    kind: {
      type: "choice",
      default: "big",
      options: [
        {
          id: "big",
          label: "Big",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "small",
          label: "Small",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
      ],
    },
    parent: null,
    requires: null,
    section: "Fake section",
    section_about: "Fake about",
    list: false,
    gives: [],
    archipelago: { ap: "offered" },
  };
  expect(summary([size], {})).toBe("Nothing extra");
  expect(summary([size], { size: "small" })).toBe("Size: Small");
  expect(chosen(size, { size: "gone" })?.id).toBe("big");
  expect(isOn(size, { size: "small" }, [size])).toBe(false);
  expect(isChanged(size, { size: "gone" })).toBe(false);
  expect(isChanged(size, { size: "small" })).toBe(true);
});

test("a toggle is changed only when its own value leaves the default", () => {
  const [on] = toggles(1) as [SettingDef];
  expect(isChanged(on, {})).toBe(false);
  expect(isChanged(on, { s0: true })).toBe(false);
  expect(isChanged(on, { s0: false })).toBe(true);
});

test("a patch sets values and puts undefined ones back to default", () => {
  const [on] = toggles(1) as [SettingDef];
  const settings = { s0: false, other: true };
  expect(patched(settings, { s1: true, other: undefined })).toEqual({
    s0: false,
    s1: true,
  });
  expect(patched(settings, defaults([on]))).toEqual({ other: true });
});

/** Toggles `all` (with its child `one`) and `x`, and the choice `size`. */
function diffSchema(): SettingDef[] {
  const def = (id: string, parent: string | null = null): SettingDef => ({
    id,
    group: "g",
    label: id.toUpperCase(),
    description: "",
    kind: { type: "toggle", default: false },
    parent,
    requires: null,
    section: "Fake section",
    section_about: "Fake about",
    list: false,
    gives: [],
    archipelago: { ap: "offered" },
  });
  const size: SettingDef = {
    ...def("size"),
    kind: {
      type: "choice",
      default: "a",
      options: [
        {
          id: "a",
          label: "Big",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "b",
          label: "Small",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "c",
          label: "Tiny",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
      ],
    },
  };
  return [def("all"), def("one", "all"), def("x"), size];
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
  const def: SettingDef = {
    ...toggles(1)[0],
    label: "Start",
    kind: { type: "inventory", default: {} },
  } as SettingDef;
  expect(isChanged(def, {})).toBe(false);
  expect(isChanged(def, { s0: {} })).toBe(false);
  const two = { s0: { hookshot: 1, bombs: 20 } };
  expect(isChanged(def, two)).toBe(true);
  expect(isOn(def, two, [def])).toBe(false);
  expect(summary([def], two)).toBe("Start: 2 items");
  expect(settingsChanges([def], {}, two)).toMatchObject([
    { before: "nothing", after: "hookshot, bombs ×20" },
  ]);
  // A swap that holds as many things is still new, and says what it was.
  const swapped = { s0: { bow: 1, bombs: 20 } };
  expect(markedChips([def], [def], swapped, two)).toEqual([
    { id: "s0", label: "Start", value: "2 items", mark: "new", was: "2 items" },
  ]);
  expect(markedChips([def], [def], two, two)).toEqual([
    { id: "s0", label: "Start", value: "2 items" },
  ]);
});

test("an unset starting inventory holds its default, and emptied it's changed", () => {
  const def: SettingDef = {
    ...toggles(1)[0],
    label: "Start",
    kind: { type: "inventory", default: { "master-sword": 1 } },
  } as SettingDef;
  expect(startOf(def, {})).toEqual({ "master-sword": 1 });
  expect(isChanged(def, {})).toBe(false);
  expect(isChanged(def, { s0: { "master-sword": 1 } })).toBe(false);
  expect(isChanged(def, { s0: {} })).toBe(true);
  expect(summary([def], {})).toBe("Start: 1 item");
});

test("a setting's value names what it gives the grid", () => {
  const met = {
    ...toggles(1)[0],
    kind: { type: "toggle", default: false },
    id: "fake-met",
    label: "Met Fake",
    gives: [["on", ["zeldas-letter"]]],
  } as SettingDef;
  const fort = {
    ...met,
    id: "fake-fort",
    label: "Fake Fort",
    kind: {
      type: "choice",
      default: "normal",
      options: [
        {
          id: "normal",
          label: "Normal",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
        {
          id: "freed",
          label: "Freed",
          forces: [],
          starts: [],
          archipelago: { ap: "offered" },
        },
      ],
    },
    gives: [["freed", ["gerudo-card"]]],
  } as SettingDef;
  const schema = [met, fort];
  expect(startGivers(schema, {})).toEqual([]);
  const given = startGivers(schema, { "fake-met": true, "fake-fort": "freed" });
  expect(given.map((g) => [g.name, g.words])).toEqual([
    ["Met Fake", ["zeldas-letter"]],
    ["Fake Fort: Freed", ["gerudo-card"]],
  ]);
});
