import {
  Box,
  FlaskConical,
  Gift,
  type LucideIcon,
  MapPin,
  Variable,
} from "lucide-react";
import { useDev } from "@/stores/dev";
import { type DevTab, useUi } from "@/stores/ui";

/** The tabs, in order, with their labels and icons. */
export const TABS: { id: DevTab; label: string; icon: LucideIcon }[] = [
  { id: "warp", label: "Warp", icon: MapPin },
  { id: "actors", label: "Actors", icon: Box },
  { id: "give", label: "Give", icon: Gift },
  { id: "values", label: "Values", icon: Variable },
  { id: "tests", label: "Tests", icon: FlaskConical },
];

/** The body's id, which each tab controls. */
export const PANEL = "dev-panel";
/** A tab's id, which labels the body while it's open. */
export const tabId = (tab: DevTab) => `dev-tab-${tab}`;

/** Opens `tab`, its list showing even while the game isn't running. */
export function openTab(tab: DevTab) {
  useUi.setPath("dev.tab", tab);
  useDev.setPath("browsing", true);
}

/** Opens Find anything. */
export const openPalette = () => useDev.setPath("palette", true);
