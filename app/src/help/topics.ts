import type { Place } from "@/stores/shell";

/** One help bubble's words, and a place in the app it can send you to. */
export interface Topic {
  title: string;
  /** One to three short sentences, one per entry. */
  body: string[];
  link?: { label: string; to: Place };
  /** The page it sits on, which the search shows it under. A field's
   * topic is named by the field instead. */
  at?: Place;
}

/** Every help text the app shows, by topic id. Plain words only: no
 * target named, nothing on getting the game, and nothing the element
 * beside it already says. */
export const TOPICS = {
  "game-dump": {
    title: "Game dump",
    body: [
      "Your own copy of the game, with the 1.1.1 update.",
      "zootr reads it to build each seed and never changes it.",
    ],
  },
  seed: {
    title: "Seeds",
    at: { tab: "Play", folder: "standard" },
    body: [
      "The same seed and settings always make the same game.",
      "Paste a friend's share code here to build exactly their seed.",
    ],
  },
  "builds-with": {
    title: "Builds with",
    at: { tab: "Play", folder: "standard" },
    body: [
      "The settings the next seed uses, a tile per group. Gold marks what changed since the last seed.",
    ],
  },
  installed: {
    title: "Installed",
    at: { tab: "Play" },
    body: [
      "The seed in the game now. The meter counts the chests this seed gave a different item.",
    ],
  },
  history: {
    title: "History",
    at: { tab: "Play", folder: "history" },
    body: [
      "Every seed you've made, newest first. Install any of them again, or open its Spoiler Log.",
    ],
  },
  "ap-slot": {
    title: "Slot",
    at: { tab: "Play", folder: "archipelago" },
    body: [
      "Your slot's file (.apznx) comes from whoever hosts the multiworld.",
      "Open it here, Install, start the game, then Connect.",
    ],
  },
  "ap-room": {
    title: "Room",
    at: { tab: "Play", folder: "archipelago" },
    body: [
      "The server address and password come from whoever hosts. Your slot name is in the slot file.",
    ],
  },
  spoiler: {
    title: "Spoiler Log",
    at: { tab: "Spoiler" },
    body: [
      "Spheres shows what you can reach in turn, each column opening the next. Way of the Hero (★) marks what the game can't be finished without.",
      "Pick a check: what it needs glows gold, dashed when one of several will do, and what it opens glows blue.",
    ],
  },
  drift: {
    title: "Changes",
    body: [
      "Settings save as you change them. The game changes only when you Generate & install.",
      "Changes count from the last seed you made. An Archipelago slot doesn't count.",
    ],
  },
  plando: {
    title: "Plando",
    at: { section: "plando" },
    body: [
      "Pin an item to a check, and the shuffle leaves it there.",
      "A check is any place that can hold an item.",
    ],
  },
  target: {
    title: "Targets",
    at: { section: "targets" },
    body: [
      "One is enough to play.",
      "Import adds a target someone shared with you, and the copy button on a card shares yours.",
    ],
  },
  "mod-folder": {
    title: "Mod folder",
    body: ["If a seed doesn't show up in game, check this folder first."],
  },
  "control-socket": {
    title: "Control socket",
    body: [
      "Lets Checks, Items and Live follow the running game, on a target that offers one.",
    ],
    link: { label: "Open Live", to: { tab: "Live" } },
  },
  variables: {
    title: "Variables",
    at: { section: "targets" },
    body: [
      "A word in braces, like {game}, becomes a path when zootr uses the field.",
      "Variables, at the foot of this page, lists them all.",
    ],
  },
  checks: {
    title: "Checks",
    at: { tab: "Checks" },
    body: [
      "They tick as you play, while the game is running.",
      "Hidden ones show once the game reports them, untracked ones never tick. Skip counts a check as done.",
    ],
  },
  items: {
    title: "Items",
    at: { tab: "Items" },
    body: [
      "Faded is missing, pips show an upgrade's level, a number counts.",
      "Pop out keeps the grid in its own window beside the game.",
    ],
  },
  "live-status": {
    title: "Live",
    at: { tab: "Live" },
    body: [
      "Live follows the game while it runs. Nothing ticks while it's closed.",
      "With autosaves only, changes show each time the game saves.",
    ],
  },
  "save-editor": {
    title: "Save Editor",
    at: { tab: "Save" },
    body: [
      "Close the game first: it writes over its saves as it plays.",
      "On Items, click raises, right-click or Backspace lowers. Presets wait for Save.",
    ],
  },
} satisfies Record<string, Topic>;

/** A topic's id. */
export type TopicId = keyof typeof TOPICS;

/** What a bubble explains: a catalog id, or a topic made where it's used. */
export type HelpTopic = TopicId | Topic;

/** Each settings group's page bubble, by group name. */
export const GROUP_HELP: Record<string, Topic> = {
  Enhancements: {
    title: "Enhancements",
    body: [
      "Comfort, cutscene skips and changes to how Link plays. Nothing here shuffles items or opens areas.",
      "Cutscenes that give an item or move Link always play.",
    ],
  },
  "Logic & access": {
    title: "Logic & access",
    body: ["How items are placed, and what stands open from the start."],
  },
  Inventory: {
    title: "Inventory",
    body: [
      "What a new game starts with. Each item leaves the shuffle, and its spot holds a Blue Rupee instead.",
      "Click an item to raise it, right-click or Backspace to lower it.",
    ],
  },
  Shuffles: {
    title: "Shuffles",
    body: ["Which places become checks, what the item pool holds, and hints."],
  },
  Dungeons: {
    title: "Dungeons",
    body: ["What a dungeon or its boss gives, and where its keys go."],
  },
};
