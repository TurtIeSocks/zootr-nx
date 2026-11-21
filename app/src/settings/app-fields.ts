import type { TopicId } from "@/help/topics";

/** One of the app's own settings: its control's id and its words, which
 * its page and the search share. */
export interface AppField {
  id: string;
  label: string;
  description: string;
  /** A help bubble beside the label. */
  help?: TopicId;
}

/** The App › Settings page's fields, in its order. */
export const APP_FIELDS = {
  game: {
    id: "game",
    label: "Game Dump",
    help: "game-dump",
    description:
      "The .nup of your own copy of the game, with the 1.1.1 update. Save extracts the files zootr reads into a cache, which takes a few seconds the first time. A folder of already extracted romfs works too: paste its path.",
  },
  gameAssets: {
    id: "game-assets",
    label: "Icons from the Game Dump",
    description:
      "Draws the item tracker's icons from your own copy of the game instead of zootr-nx's. zootr-nx doesn't include any of the game's art: the icons are read from your game dump, on this computer",
  },
  updateCheck: {
    id: "update-check",
    label: "Check for Updates",
    description:
      "Asks GitHub for the latest release at launch and every 6 hours, and the sidebar says when a newer one is out. Off, the app never contacts GitHub.",
  },
  devTools: {
    id: "dev-tools",
    label: "Developer Tools",
    description:
      "Shows the dev panel, Test Setup and the targets' fields for scripted play. Off, a hidden setup leaves your seeds alone.",
  },
} satisfies Record<string, AppField>;
