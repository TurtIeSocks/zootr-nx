import type { Choice, SettingDef } from "@/bindings";
import type { Settings } from "@/lib/config";
import { parentOn } from "@/lib/settings";
import { useConfig } from "@/stores/config";

/** How a description of a setting nobody has tested yet starts. */
const UNTESTED = "Untested. ";

/** A description without its "Untested." prefix, and whether it had one. */
export const untested = (description: string): [boolean, string] =>
  description.startsWith(UNTESTED)
    ? [true, description.slice(UNTESTED.length)]
    : [false, description];

/** The parent forcing a setting on, if one is. */
export const lockedBy = (
  def: SettingDef,
  schema: SettingDef[],
  settings: Settings,
) =>
  parentOn(def, settings, schema)
    ? schema.find((d) => d.id === def.parent)
    : undefined;

/** The parent forcing `def` on, if one is, kept up to date. */
export const useLockedBy = (def: SettingDef) =>
  useConfig((s) => lockedBy(def, s.schema, s.config.settings));

/** The id of a setting's control. */
export const controlId = (def: SettingDef) => `setting-${def.id}`;
/** The id of a setting's label, which names a control with no id of its
 * own. */
export const labelId = (def: SettingDef) => `${controlId(def)}-label`;

/** At most this many options show in place, side by side. */
const SEGMENTS = 3;
/** The longest option label that still fits side by side. */
const SEGMENT_LABEL = 14;

/** Whether a choice's options are few and short enough to show in place. */
export const segmented = (options: Choice[]) =>
  options.length <= SEGMENTS &&
  options.every((o) => o.label.length <= SEGMENT_LABEL);
