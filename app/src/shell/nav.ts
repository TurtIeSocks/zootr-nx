import {
  Activity,
  Backpack,
  Castle,
  Clapperboard,
  FlaskConical,
  Gamepad2,
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
} from "lucide-react";

/** The views that aren't settings. */
export type Tab = "Play" | "Save" | "Checks" | "Items" | "Live" | "Spoiler";

/** A tab's name where it isn't its id. */
const TAB_LABELS: Partial<Record<Tab, string>> = { Save: "Save Editor" };

/** What the sidebar and breadcrumb call `tab`. */
export const tabLabel = (tab: Tab) => TAB_LABELS[tab] ?? tab;

/** Play and Save, at the top of the sidebar. */
export const TOP_TABS: [Tab, LucideIcon][] = [
  ["Play", Gamepad2],
  ["Save", Save],
];

/** The installed seed's views, under Current run. */
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
  ["test", "Test setup", FlaskConical],
];

/** The randomizer's own panes, after the schema's groups. */
export const RANDOMIZER_PANES: [Section, string, LucideIcon][] = [
  ["plando", "Plando", Pin],
];

export const appPane = (section: Section) =>
  APP_PANES.find(([id]) => id === section);

/** One of the app's or the randomizer's own panes, not a schema group. */
export const ownPane = (section: Section) =>
  [...APP_PANES, ...RANDOMIZER_PANES].find(([id]) => id === section);

/** Groups the schema has, or may grow. Any other group gets `Settings2`. */
const GROUP_ICONS: Record<string, LucideIcon> = {
  General: SlidersHorizontal,
  Cutscenes: Clapperboard,
  "Logic & access": Route,
  Inventory: Package,
  Shuffles: Shuffle,
  Dungeons: Castle,
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
