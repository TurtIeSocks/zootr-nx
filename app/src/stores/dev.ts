import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  commands,
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
  /** The window floats over the game. */
  pinned: boolean;
}

export const useDev = create<Dev>()(
  dotPath(
    (): Dev => ({
      game: {},
      ready: false,
      gameValue: "",
      options: { places: [], spawns: [], items: [], names: [], quests: {} },
      tests: { setups: {}, goals: {}, holds: {} },
      pinned: true,
    }),
  ),
);

useDev.subscribePath("game.state", (state) =>
  useDev.setPath("ready", state !== undefined),
);
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
  const saved = config && full(config).saved_tests;
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

/** Keeps the window over the game, or lets it drop behind. */
export async function pin() {
  const pinned = useDev.getPath("pinned");
  await getCurrentWindow().setAlwaysOnTop(!pinned);
  useDev.setPath("pinned", !pinned);
}
