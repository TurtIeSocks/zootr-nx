import { expect, test } from "vitest";
import type { SettingDef } from "@/bindings";
import {
  chosen,
  defaults,
  isChanged,
  isOn,
  patched,
  settingsDiff,
  summary,
  summaryParts,
} from "@/lib/settings";

const toggles = (n: number): SettingDef[] =>
  Array.from({ length: n }, (_, i) => ({
    id: `s${i}`,
    group: "g",
    label: `S${i}`,
    description: "",
    kind: { type: "toggle", default: true },
    parent: null,
    subgroup: null,
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
    subgroup: null,
  });
  const child = off("s1", "s0");
  const schema = [off("s0", null), child, off("s2", "s0")];
  expect(isOn(child, { s1: false }, schema)).toBe(false);
  expect(isOn(child, { s0: true, s1: false }, schema)).toBe(true);
  expect(summary(schema, { s0: true })).toBe("S0");
  expect(summary(schema, { s2: true })).toBe("S2");
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
        { id: "big", label: "Big" },
        { id: "small", label: "Small" },
      ],
    },
    parent: null,
    subgroup: null,
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

test("a diff lists effective changes, with a parent standing in", () => {
  const def = (id: string, parent: string | null = null): SettingDef => ({
    id,
    group: "g",
    label: id.toUpperCase(),
    description: "",
    kind: { type: "toggle", default: false },
    parent,
    subgroup: null,
  });
  const size: SettingDef = {
    ...def("size"),
    kind: {
      type: "choice",
      default: "a",
      options: [
        { id: "a", label: "Big" },
        { id: "b", label: "Small" },
      ],
    },
  };
  const schema = [def("all"), def("one", "all"), def("x"), size];
  expect(settingsDiff(schema, {}, { x: false, size: "a" })).toEqual([]);
  expect(settingsDiff(schema, {}, { x: true, size: "b" })).toEqual([
    "X: off → on",
    "SIZE: Big → Small",
  ]);
  expect(settingsDiff(schema, { one: true }, { all: true })).toEqual([
    "ALL: off → on",
  ]);
});

test("a starting inventory is changed while it holds anything", () => {
  const def: SettingDef = {
    ...toggles(1)[0],
    label: "Start",
    kind: { type: "inventory" },
  } as SettingDef;
  expect(isChanged(def, {})).toBe(false);
  expect(isChanged(def, { s0: {} })).toBe(false);
  const two = { s0: { hookshot: 1, bombs: 20 } };
  expect(isChanged(def, two)).toBe(true);
  expect(isOn(def, two, [def])).toBe(false);
  expect(summary([def], two)).toBe("Start: 2 items");
  expect(settingsDiff([def], {}, two)).toEqual([
    "Start: nothing → hookshot, bombs ×20",
  ]);
});
