import type { Archipelago, Choice, SettingDef } from "@/bindings";
import type { Settings } from "@/lib/config";
import { forcer, unmet } from "@/lib/settings";
import { useConfig } from "@/stores/config";

/** How a description of a setting nobody has tested yet starts. */
const UNTESTED = "Untested. ";

/** A description without its "Untested." prefix, and whether it had one. */
export const untested = (description: string): [boolean, string] =>
  description.startsWith(UNTESTED)
    ? [true, description.slice(UNTESTED.length)]
    : [false, description];

/** How the app marks a setting or pick the Archipelago world doesn't
 * offer: its badge, and why. */
export interface ApMark {
  label: "Not in AP" | "Forced in AP";
  why: string;
}

/** The mark for a setting or pick the Archipelago world leaves out, or
 * nothing while it offers it. */
export const apMark = (ap: Archipelago): ApMark | undefined => {
  switch (ap.ap) {
    case "offered":
      return undefined;
    case "later":
      return { label: "Not in AP", why: `Not in Archipelago yet (${ap.why})` };
    case "never":
      return {
        label: "Not in AP",
        why: `Doesn't apply to Archipelago: ${ap.why}`,
      };
    case "on-until":
      return {
        label: "Forced in AP",
        why: `Always on in Archipelago for now (${ap.why})`,
      };
    case "always-on":
      return {
        label: "Forced in AP",
        why: `Always on in Archipelago: ${ap.why}`,
      };
  }
};

/** A choice's picks the Archipelago world leaves out, by their mark's
 * why. */
export const heldPicks = (def: SettingDef): [ApMark, string[]][] => {
  if (def.kind.type !== "choice") return [];
  const by = new Map<string, [ApMark, string[]]>();
  for (const o of def.kind.options) {
    const mark = apMark(o.archipelago);
    if (mark)
      by.set(mark.why, [mark, [...(by.get(mark.why)?.[1] ?? []), o.label]]);
  }
  return [...by.values()];
};

/** The setting forcing `def` on, if one is. */
export const lockedBy = (
  def: SettingDef,
  schema: SettingDef[],
  settings: Settings,
) => forcer(def, settings, schema);

/** The setting forcing `def` on, if one is, kept up to date. */
export const useLockedBy = (def: SettingDef) =>
  useConfig((s) => lockedBy(def, s.schema, s.config.settings));

/** The toggle `def` needs on, while it's off, kept up to date. */
export const useUnmet = (def: SettingDef) =>
  useConfig((s) => unmet(def, s.config.settings, s.schema));

/** The id of a setting's control. */
export const controlId = (def: SettingDef) => `setting-${def.id}`;
/** The id of a setting's label, which names a control with no id of its
 * own. */
export const labelId = (def: SettingDef) => `${controlId(def)}-label`;

/** At most this many options show in place, side by side. More take a
 * select. */
const SEGMENTS = 2;
/** The longest option label that still fits side by side. */
const SEGMENT_LABEL = 14;

/** Whether a choice's options are few and short enough to show in place. */
export const segmented = (options: Choice[]) =>
  options.length <= SEGMENTS &&
  options.every((o) => o.label.length <= SEGMENT_LABEL);
