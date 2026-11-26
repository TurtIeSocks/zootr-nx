import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { commands, type Summary } from "@/bindings";
import type { Edit } from "@/checks/rows";
import { call } from "@/lib/api";
import { DUMP_GONE, NEED_DUMP, NEED_TARGET } from "@/lib/config";
import type { Section, Tab } from "@/shell/nav";
import { useConfig } from "@/stores/config";
import { type PlayTab, useUi } from "@/stores/ui";

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
  /** How many unsaved bars show. The drift bar steps aside for them. */
  unsaved?: number;
  /** The search over the whole app is open. */
  find?: boolean;
  /** How many History rows are drawn, a page more each time it's asked. */
  historyDrawn?: number;
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

/** A place in the app a link can open: a settings section, or a tab and,
 * on Play, one of its folders. */
export type Place = { section: Section } | { tab: Tab; folder?: PlayTab };

/** Opens `place`, the way the sidebar does. */
export function go(place: Place) {
  if ("section" in place) return openSettings(place.section);
  if (place.folder) useUi.setPath("play.tab", place.folder);
  useUi.setPath("tab", place.tab);
}

/** Opens Settings with the game dump field focused. */
export function openDumpField() {
  openSettings("settings");
  // The page draws on the next frame.
  setTimeout(() => document.getElementById("game")?.focus(), 50);
}

/** Each reason nothing can be built or installed that a page fixes, and
 * how to open that page. */
const FIXES = new Map<string, () => void>([
  [NEED_DUMP, openDumpField],
  [DUMP_GONE, openDumpField],
  [NEED_TARGET, () => openSettings("targets")],
]);

/** Whether a page fixes `why`, a reason nothing can be built or installed. */
export const fixable = (why: string) => FIXES.has(why);

/** Opens where `why`, a reason nothing can be built or installed, is fixed.
 * False for a reason no page fixes. */
export function fix(why: string): boolean {
  const open = FIXES.get(why);
  open?.();
  return !!open;
}

/** Counts an unsaved bar in while it shows, so the drift bar steps aside.
 * Returns the effect's cleanup, which counts it back out. */
export function claimFoot() {
  useShell.setPath("unsaved", (n = 0) => n + 1);
  return () => useShell.setPath("unsaved", (n = 0) => n - 1);
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
