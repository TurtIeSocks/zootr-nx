import type { Place } from "@/stores/shell";

/** One help bubble's words, and a place in the app it can send you to. */
export interface Topic {
  title: string;
  /** One to three short sentences, one per entry. */
  body: string[];
  link?: { label: string; to: Place };
}

/** Every help text the app shows, by topic id. Plain words only: no
 * target named, nothing on getting the game. */
export const TOPICS = {
  "game-dump": {
    title: "Game dump",
    body: [
      "Your own copy of the game, with the 1.1.1 update.",
      "zootr reads it to build each seed and never changes it.",
    ],
  },
  target: {
    title: "Targets",
    body: [
      "Where you play: the folder the game loads mods from, and what Play opens.",
      "Install writes each seed there. One is enough.",
    ],
  },
  "mod-folder": {
    title: "Mod folder",
    body: [
      "The folder the game loads this mod from.",
      "If a seed doesn't show up in game, check this first.",
    ],
  },
  "control-socket": {
    title: "Control socket",
    body: [
      "Lets Checks, Items and Live follow the running game, on a target that offers one.",
    ],
    link: { label: "Open Live", to: { tab: "Live" } },
  },
} satisfies Record<string, Topic>;

/** A topic's id. */
export type TopicId = keyof typeof TOPICS;
