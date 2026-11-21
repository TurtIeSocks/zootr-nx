import type { Choice, SettingDef, StartInventory } from "@/bindings";
import type { Settings } from "@/lib/config";
import { count, LISTED } from "@/lib/format";

/** Whether a setting is on: its own value or default, or forced on. */
export function isOn(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): boolean {
  if (forcedOn(def, settings, schema)) return true;
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

/** A patch turning every toggle of `defs` on (or off), leaving out one a
 * parent forces on and one already there. Empty when nothing would
 * change. */
export function setToggles(
  defs: SettingDef[],
  schema: SettingDef[],
  settings: Settings,
  on: boolean,
): Patch {
  return Object.fromEntries(
    defs
      .filter(
        (d) =>
          d.kind.type === "toggle" &&
          !forcedOn(d, settings, schema) &&
          isOn(d, settings, schema) !== on,
      )
      .map((d) => [d.id, on]),
  );
}

/** The setting forcing `def` on, if one is: its parent while that's on,
 * or a choice whose pick forces it. */
export function forcer(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): SettingDef | undefined {
  const parent = schema.find((d) => d.id === def.parent);
  if (parent !== undefined && isOn(parent, settings, schema)) return parent;
  return schema.find((d) => chosen(d, settings)?.forces.includes(def.id));
}

/** Whether a setting is forced on, by its parent or a choice's pick. */
export const forcedOn = (
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
) => forcer(def, settings, schema) !== undefined;

/** The choices whose picks give start inventory words, each with them. */
export const startForcers = (schema: SettingDef[], settings: Settings) =>
  schema.flatMap((def) => {
    const words = chosen(def, settings)?.starts ?? [];
    return words.length > 0 ? [{ def, words }] : [];
  });

/** `start` holding each of `words` at least once. */
export function withForced(
  start: StartInventory,
  words: string[],
): StartInventory {
  return Object.fromEntries([
    ...words.map((w) => [w, 1] as const),
    ...Object.entries(start),
  ]);
}

/** A setting that's on, in a summary's words: a toggle's label, or a
 * choice's label and its pick, or an inventory's and how much it holds. */
export interface Chip {
  id: string;
  label: string;
  /** A choice's pick, or how many things an inventory holds. */
  value?: string;
}

/** A chip as one phrase: "Label" or "Label: Pick". */
export const chipText = ({ label, value }: Chip) =>
  value === undefined ? label : `${label}: ${value}`;

/** The chip of a toggle that is on, a choice off its default, or an
 * inventory holding anything. A toggle
 * its parent turns on has none: the parent stands in. */
function chipOf(
  def: SettingDef,
  settings: Settings,
  schema: SettingDef[],
): Chip | null {
  const { id, label } = def;
  if (def.kind.type === "choice") {
    const pick = chosen(def, settings);
    return pick && pick.id !== def.kind.default
      ? { id, label, value: pick.label }
      : null;
  }
  if (def.kind.type === "inventory") {
    const n = held(def, settings);
    return n > 0 ? { id, label, value: count(n, "item") } : null;
  }
  return isOn(def, settings, schema) && !forcedOn(def, settings, schema)
    ? { id, label }
    : null;
}

/** The chips of every setting in `defs` that's on. */
const chips = (defs: SettingDef[], settings: Settings, schema: SettingDef[]) =>
  defs.flatMap((d) => chipOf(d, settings, schema) ?? []);

/** The labels of every setting that changed, or how many when it's long. A
 * parent stands in for its children. */
export function summaryParts(
  schema: SettingDef[],
  settings: Settings,
): string[] {
  const on = chips(schema, settings, schema).map(chipText);
  if (on.length === 0) return ["Nothing extra"];
  return on.length > LISTED ? [`${on.length} settings on`] : on;
}

/** The labels of every setting that changed, as one line. */
export function summary(schema: SettingDef[], settings: Settings): string {
  return summaryParts(schema, settings).join(", ");
}

/** A chip marked against the settings it's compared with: `new` when they
 * lacked it or picked otherwise (`was` holds their pick), `gone` when they
 * had it and these don't. */
export interface MarkedChip extends Chip {
  mark?: "new" | "gone";
  was?: string;
}

/** The chips of `defs` under `settings`, each marked against `base`'s, in
 * schema order. A chip `base` had that a parent now stands in for isn't
 * gone. Without a base, nothing is marked. */
export function markedChips(
  defs: SettingDef[],
  schema: SettingDef[],
  settings: Settings,
  base: Settings | null,
): MarkedChip[] {
  return defs.flatMap((d): MarkedChip[] => {
    const now = chipOf(d, settings, schema);
    if (base === null) return now ? [now] : [];
    const then = chipOf(d, base, schema);
    if (now === null) {
      return then && !forcedOn(d, settings, schema)
        ? [{ ...then, mark: "gone" }]
        : [];
    }
    if (then === null) return [{ ...now, mark: "new" }];
    return shown(d, base, schema) === shown(d, settings, schema)
      ? [now]
      : [{ ...now, mark: "new", was: then.value }];
  });
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

/** A setting that differs between two settings, as words. */
export interface Change {
  def: SettingDef;
  before: string;
  after: string;
}

/** Every setting that differs between `before` and `after`. A parent that
 * changed stands in for its children. */
export function settingsChanges(
  schema: SettingDef[],
  before: Settings,
  after: Settings,
): Change[] {
  return schema.flatMap((def) => {
    if (forcedOn(def, before, schema) !== forcedOn(def, after, schema)) {
      return [];
    }
    const [a = "", b = ""] = [before, after].map((s) => shown(def, s, schema));
    return a === b ? [] : [{ def, before: a, after: b }];
  });
}
