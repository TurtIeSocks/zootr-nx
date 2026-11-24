import {
  Activity,
  Backpack,
  FileText,
  FlaskConical,
  Gamepad2,
  Globe,
  Heart,
  History,
  Lightbulb,
  ListChecks,
  type LucideIcon,
  MonitorPlay,
  Package,
  Pin,
  Route,
  Save,
  ScrollText,
  Settings,
  Settings2,
  Shuffle,
  SlidersHorizontal,
  Sparkles,
  Swords,
} from "lucide-react";

import type { PlayTab } from "@/stores/ui";

/** The views that aren't settings. */
export type Tab = "Play" | "Save" | "Checks" | "Items" | "Live" | "Spoiler";

/** A tab's name where it isn't its id. */
const TAB_LABELS: Partial<Record<Tab, string>> = {
  Save: "Save Editor",
  Spoiler: "Spoiler Log",
};

/** What the sidebar and breadcrumb call `tab`. */
export const tabLabel = (tab: Tab) => TAB_LABELS[tab] ?? tab;

/** Play and Save, at the top of the sidebar. */
export const TOP_TABS: [Tab, LucideIcon][] = [
  ["Play", Gamepad2],
  ["Save", Save],
];

/** The Play page's folders, in order: each one's label and icon. */
export const PLAY_FOLDERS: Record<
  PlayTab,
  { label: string; icon: LucideIcon }
> = {
  standard: { label: "Standard", icon: Swords },
  archipelago: { label: "Archipelago", icon: Globe },
  history: { label: "History", icon: History },
  favorites: { label: "Favorites", icon: Heart },
};

/** The installed seed's views, under Installed Seed. */
export const RUN_TABS: [Tab, LucideIcon][] = [
  ["Checks", ListChecks],
  ["Items", Backpack],
  ["Live", Activity],
  ["Spoiler", ScrollText],
];

/** A schema group name, or one of the app's own panes. */
export type Section = string;

/** The app's own panes, by section id. */
export const APP_PANES: [Section, string, LucideIcon][] = [
  ["settings", "Settings", Settings],
  ["targets", "Targets", MonitorPlay],
  ["logs", "Logs", FileText],
  ["test", "Test Setup", FlaskConical],
];

/** The randomizer's own panes, after the schema's groups. */
export const RANDOMIZER_PANES: [Section, string, LucideIcon][] = [
  ["plando", "Plando", Pin],
];

/** Panes that fill the window and scroll their own rows. */
export const FILL_PANES: ReadonlySet<Section> = new Set(["logs"]);

/** Panes only developer tools show. */
const DEV_PANES: ReadonlySet<Section> = new Set(["test"]);

/** Whether `section` shows, with developer tools on or off. */
export const shows = (section: Section, devTools: boolean) =>
  devTools || !DEV_PANES.has(section);

export const appPane = (section: Section) =>
  APP_PANES.find(([id]) => id === section);

/** One of the app's or the randomizer's own panes, not a schema group. */
export const ownPane = (section: Section) =>
  [...APP_PANES, ...RANDOMIZER_PANES].find(([id]) => id === section);

/** Groups the schema has, or may grow. Any other group gets `Settings2`. */
const GROUP_ICONS: Record<string, LucideIcon> = {
  Enhancements: SlidersHorizontal,
  "Logic & Access": Route,
  Inventory: Package,
  Shuffles: Shuffle,
  Hints: Lightbulb,
  Tricks: Sparkles,
};

export const groupIcon = (group: string) => GROUP_ICONS[group] ?? Settings2;

/** Where a page sits, for the topbar. */
export const crumbs = (tab: Tab | "Settings", section: Section): string[] =>
  tab !== "Settings"
    ? [tabLabel(tab)]
    : [
        appPane(section) ? "App" : "Randomizer",
        ownPane(section)?.[1] ?? section,
      ];
