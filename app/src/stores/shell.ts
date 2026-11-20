import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { commands, type Summary } from "@/bindings";
import type { Edit } from "@/checks/rows";
import { call } from "@/lib/api";
import type { Section, Tab } from "@/shell/nav";
import { useConfig } from "@/stores/config";
import { useUi } from "@/stores/ui";

/** The main window's moments: what's open, and what was sent where. */
interface Shell {
  /** The seed whose spoiler History opened, as it was when clicked. */
  spoiler?: Summary;
  /** The installed seed whose spoiler was revealed. */
  revealed?: number;
  /** The seed being labelled, as it was when clicked, so a poll never
   * clobbers a label being typed. */
  labeling?: Summary;
  /** The check whose note or name is being edited. Kept by hash, so the
   * dialog follows the row through a reload and outlives its filter. */
  checkText?: { hash: string; edit: Edit };
}

/** The shell store. Not saved: every launch starts with nothing open. */
export const useShell = create<Shell>()(dotPath((): Shell => ({})));

/** Opens Settings at `section`, or at the first settings group. */
export function openSettings(section?: Section) {
  useUi.setPath(
    "section",
    section ?? useConfig.getState().groups[0]?.name ?? "settings",
  );
  useUi.setPath("tab", "Settings");
}

/** A place in the app a link can open: a settings section or a tab. */
export type Place = { section: Section } | { tab: Tab };

/** Opens `place`, the way the sidebar does. */
export function go(place: Place) {
  if ("section" in place) return openSettings(place.section);
  useUi.setPath("tab", place.tab);
}

/** Opens Settings at Targets when there are none, since nothing can be
 * installed without one. */
export function openTargetsIfNone() {
  const { loaded, config } = useConfig.getState();
  if (loaded && config.targets.length === 0) openSettings("targets");
}

/** Opens the dev panel in its own window. */
export const openDev = () => call(commands.devPopOut());

/** Opens the spoiler of History's `entry`, as it is now, in a dialog. */
export const openSpoiler = (entry: Summary) =>
  useShell.setPath("spoiler", entry);

/** Closes the spoiler sheet. */
export const closeSpoiler = () => useShell.setPath("spoiler", undefined);

/** Opens the label dialog on `entry`, as it is now. */
export const openLabel = (entry: Summary) =>
  useShell.setPath("labeling", entry);

/** Closes the label dialog. */
export const closeLabel = () => useShell.setPath("labeling", undefined);

/** Opens the text dialog on a check's note or name. */
export const openCheckText = (hash: string, edit: Edit) =>
  useShell.setPath("checkText", { hash, edit });

/** Closes the check text dialog. */
export const closeCheckText = () => useShell.setPath("checkText", undefined);
