import type { Choice, SettingDef, StartInventory } from "@/bindings";
import type { Settings } from "@/lib/config";
import { count, LISTED } from "@/lib/format";

/** Whether a setting is on: its own value or default, or its parent's. */
export function isOn(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): boolean {
  if (parentOn(def, settings, schema)) return true;
  const value = settings[def.id];
  if (def.kind.type !== "toggle") return false;
  return typeof value === "boolean" ? value : def.kind.default;
}

/** A choice's picked option, or its default when unset or unknown. */
export function chosen(
  def: SettingDef,
  settings: Settings,
): Choice | undefined {
  if (def.kind.type !== "choice") return undefined;
  const { default: fallback, options } = def.kind;
  const value = settings[def.id];
  return (
    options.find((o) => o.id === value) ??
    options.find((o) => o.id === fallback)
  );
}

/** A starting inventory's words and counts, empty when unset. */
export function startOf(value: Settings[string] | undefined): StartInventory {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return {};
  }
  return Object.fromEntries(
    Object.entries(value).filter(
      (e): e is [string, number] => typeof e[1] === "number",
    ),
  );
}

/** How many things a starting inventory holds, each word once. */
const held = (def: SettingDef, settings: Settings) =>
  Object.keys(startOf(settings[def.id])).length;

/** Whether a setting's own value differs from its default. */
export function isChanged(def: SettingDef, settings: Settings): boolean {
  if (def.kind.type === "choice") {
    return chosen(def, settings)?.id !== def.kind.default;
  }
  if (def.kind.type === "inventory") return held(def, settings) > 0;
  const value = settings[def.id];
  return typeof value === "boolean" && value !== def.kind.default;
}

/** New values by setting id. `undefined` puts a setting back to its
 * default. */
export type Patch = Record<
  string,
  boolean | string | StartInventory | undefined
>;

/** `settings` with `patch` applied. */
export function patched(settings: Settings, patch: Patch): Settings {
  const next: Settings = { ...settings };
  for (const [id, value] of Object.entries(patch)) {
    if (value === undefined) delete next[id];
    else next[id] = value;
  }
  return next;
}

/** A patch putting every one of `defs` back at its default. */
export const defaults = (defs: SettingDef[]): Patch =>
  Object.fromEntries(defs.map((d) => [d.id, undefined]));

/** Whether a setting is forced on by its parent. */
export function parentOn(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): boolean {
  const parent = schema.find((d) => d.id === def.parent);
  return parent !== undefined && isOn(parent, settings, schema);
}

/** The label of a toggle that is on, or a choice off its default. */
function changed(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): string[] {
  if (def.kind.type === "choice") {
    const pick = chosen(def, settings);
    return pick && pick.id !== def.kind.default
      ? [`${def.label}: ${pick.label}`]
      : [];
  }
  if (def.kind.type === "inventory") {
    const n = held(def, settings);
    return n > 0 ? [`${def.label}: ${count(n, "item")}`] : [];
  }
  return isOn(def, settings, schema) && !parentOn(def, settings, schema)
    ? [def.label]
    : [];
}

/** The labels of every setting that changed, or how many when it's long. A
 * parent stands in for its children. */
export function summaryParts(
  schema: SettingDef[],
  settings: Settings,
): string[] {
  const on = schema.flatMap((d) => changed(d, settings, schema));
  if (on.length === 0) return ["Nothing extra"];
  return on.length > LISTED ? [`${on.length} settings on`] : on;
}

/** The labels of every setting that changed, as one line. */
export function summary(schema: SettingDef[], settings: Settings): string {
  return summaryParts(schema, settings).join(", ");
}

/** A setting's effective value as a word: a choice's label, or on/off. */
function shown(def: SettingDef, settings: Settings, schema: SettingDef[]) {
  if (def.kind.type === "choice") return chosen(def, settings)?.label ?? "";
  if (def.kind.type === "inventory") {
    const start = startOf(settings[def.id]);
    const words = Object.entries(start).map(([w, n]) =>
      n === 1 ? w : `${w} ×${n}`,
    );
    return words.length > 0 ? words.join(", ") : "nothing";
  }
  return isOn(def, settings, schema) ? "on" : "off";
}

/** One line per setting that differs between `before` and `after`. A parent
 * that changed stands in for its children. */
export function settingsDiff(
  schema: SettingDef[],
  before: Settings,
  after: Settings,
): string[] {
  return schema.flatMap((d) => {
    if (parentOn(d, before, schema) !== parentOn(d, after, schema)) return [];
    const [a, b] = [before, after].map((s) => shown(d, s, schema));
    return a === b ? [] : [`${d.label}: ${a} → ${b}`];
  });
}
