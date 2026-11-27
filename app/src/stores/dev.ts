import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  commands,
  type DevNear,
  type DevState,
  type DevSwitches,
  type GameOptions,
  type Goal,
} from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { type FullTest, full, fullTest } from "@/lib/config";
import { once } from "@/stores/once";

/** How often the game and the saved setups are read again. */
const POLL_MS = 1000;

/** What the game is doing, or why it can't be read. */
interface Game {
  state?: DevState;
  error?: string;
}

/** Live actors' last read: the live placed actors by distance from Link
 * then, kept in that order until the next read. */
export type LiveRead =
  | { status: "loading" }
  | { status: "ok"; map: string; list: DevNear[]; at: number }
  | { status: "error"; error: string };

/** An actor Live actors hid, hidden again after each load of its map. */
export interface HiddenActor {
  hash: string;
  actor: string;
  label: string | null;
  map: string;
}

/** What a request is for, which picks its entry's icon. */
export type Kind =
  | "warp"
  | "hide"
  | "age"
  | "switch"
  | "give"
  | "step"
  | "set"
  | "apply"
  | "play";

/** One request's result, as the activity line shows it. */
export interface Entry {
  kind: Kind;
  /** False when it failed. */
  ok: boolean;
  /** What the command said, or why it failed. Apply's has a line per
   * thing it did. */
  text: string;
  /** When it ended, in ms since the epoch. */
  at: number;
}

/** How many results the activity history keeps. */
export const HISTORY = 20;

/** The dev panel's window: the game, reread every second, and what it
 * offers. */
interface Dev {
  game: Game;
  /** The game can be read, so requests can go. */
  ready: boolean;
  options: GameOptions;
  /** Why the options didn't load. */
  error?: string;
  tests: {
    /** The saved test setups, by name, once read. Read and written whole,
     * since a name may hold a dot. */
    setups: Record<string, FullTest>;
    /** The setups' names, sorted. */
    names?: string[];
    /** Each setup's goals as the game holds them. Unread while the game
     * can't be: the status already says why. */
    goals: Partial<Record<string, Goal[]>>;
    /** The goals that already held when each setup was last applied, by
     * name. */
    holds: Partial<Record<string, string[]>>;
  };
  /** The GameData value picked in Set. */
  gameValue: string;
  /** Live actors: none of it kept once the app closes. */
  live: {
    read: LiveRead;
    hidden: HiddenActor[];
    /** Read again every few seconds as Link moves. */
    follow: boolean;
    query: string;
  };
  /** Every request's result, and the one running: one at a time across
   * the window. */
  activity: {
    /** The last `HISTORY` results, newest first. */
    log: Entry[];
    /** What the running request does, while one runs. */
    running: { kind: Kind; what: string } | null;
  };
  /** The map and spot of the last teleport that worked. */
  lastWarp: { map: string; spot: string } | null;
  /** The target Play starts: the one picked in the main window. */
  target: string | null;
  /** Find anything is open. */
  palette: boolean;
  /** A tab was picked since the game stopped being read, so its list
   * shows in place of the not-running note. */
  browsing: boolean;
}

export const useDev = create<Dev>()(
  dotPath(
    (): Dev => ({
      game: {},
      ready: false,
      gameValue: "",
      live: {
        read: { status: "loading" },
        hidden: [],
        follow: false,
        query: "",
      },
      options: { places: [], spawns: [], items: [], names: [], quests: {} },
      tests: { setups: {}, goals: {}, holds: {} },
      activity: { log: [], running: null },
      lastWarp: null,
      target: null,
      palette: false,
      browsing: false,
    }),
  ),
);

useDev.subscribePath("game.state", (state) =>
  useDev.setPath("ready", state !== undefined),
);
// The note shows again the next time the game stops.
useDev.subscribePath("ready", (ready) => {
  if (ready) useDev.setPath("browsing", false);
});
useDev.subscribePath("tests.setups", (setups) => {
  if (!setups) return;
  useDev.setPath("tests.names", Object.keys(setups).sort());
});

/** Reads what the game is doing, or why it can't be read. */
async function readGame() {
  try {
    const state = await call(commands.devState());
    useDev.setPath("game", keep<Game>({ state }));
  } catch (e) {
    useDev.setPath("game", keep<Game>({ error: message(e) }));
  }
}

/** Reads the saved setups again, so edits in Settings show. A failed read
 * keeps the last ones. */
async function readTests() {
  const config = await call(commands.config()).catch(() => undefined);
  const goals = await call(commands.devGoals()).catch(() => ({}));
  const cfg = config && full(config);
  const saved = cfg?.saved_tests;
  if (cfg) useDev.setPath("target", cfg.picked ?? cfg.targets[0]?.name ?? null);
  const setups = saved
    ? Object.fromEntries(
        Object.entries(saved).map(([n, t]) => [n, fullTest(t)]),
      )
    : useDev.getState().tests.setups;
  useDev.setPath("tests.setups", keep(setups));
  useDev.setPath("tests.goals", keep(goals));
}

/** One poll: the game and the saved setups. */
const read = () => Promise.all([readGame(), readTests()]);

/** Loads the options once, then polls every `POLL_MS` until stopped. */
const started = once(async () => {
  call(commands.gameOptions())
    .then((options) => useDev.setPath("options", options))
    .catch((e) => useDev.setPath("error", message(e)));
  void read();
  const id = setInterval(read, POLL_MS);
  return () => clearInterval(id);
});

export const { init, stop } = started;

/**
 * Runs `req` unless a request is already running: one at a time across the
 * window. `what` names it as a verb phrase ("warp to X"), shown while it
 * runs and in why it failed. What it returns, or why it failed, becomes the
 * activity line's newest entry. Resolves true when it worked.
 */
export async function send(
  kind: Kind,
  what: string,
  req: () => Promise<string>,
): Promise<boolean> {
  if (useDev.getPath("activity.running")) return false;
  useDev.setPath("activity.running", { kind, what });
  let entry: Omit<Entry, "at">;
  try {
    entry = { kind, ok: true, text: await req() };
  } catch (e) {
    entry = { kind, ok: false, text: `Couldn't ${what}: ${message(e)}` };
  }
  useDev.setPath("activity", ({ log }) => ({
    log: [{ ...entry, at: Date.now() }, ...log].slice(0, HISTORY),
    running: null,
  }));
  return entry.ok;
}

/** Whether a request can go now: the game can be read and none is
 * running. */
export function useCanSend() {
  const [ready] = useDev.usePath("ready");
  const [running] = useDev.usePath("activity.running");
  return ready && !running;
}

/** Sets one switch, keeping the rest as the game last held them, and shows
 * what the game read back. */
export async function setSwitches(name: keyof DevSwitches, value: boolean) {
  const now = await call(
    commands.devSwitches({
      ...useDev.getPath("game.state.switches"),
      [name]: value,
    }),
  );

  if (useDev.getPath("game.state")) {
    useDev.setPath("game.state.switches", now);
  }
  return now;
}
