import { vi } from "vitest";
import type { DevState, DevSwitches, DevValue, Goal } from "@/bindings";

/** The dev window's tests' stand-in for the game and the backend. Each
 * test file mocks `@/bindings` with `bindings` and the window module with
 * `fakeWindow`, and calls `fakeGame` (from `dev-window`) once. Nothing here
 * imports what imports `@/bindings`, so the mock can load it. */

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: string };
export const ok = <T>(data: T): Promise<Result<T>> =>
  Promise.resolve({ status: "ok", data });
export const err = <T>(error: string): Promise<Result<T>> =>
  Promise.resolve({ status: "error", error });

/** What the game is doing, or undefined while it isn't running. */
export const game: { state: DevState | undefined } = { state: undefined };

export const devTeleport = vi.fn((_p: string) => ok("sent"));
const placed = (actor: string, hash: string, label: string | null = null) => ({
  actor,
  hash,
  label,
});
export const devActors = vi.fn((_map: string) =>
  ok([
    placed("Npc_A", "1"),
    placed("Obj_A", "2", "Field_000"),
    placed("Obj_A", "3"),
  ]),
);
export const devWarp = vi.fn((actor: string, _side: string) =>
  ok(`warped beside ${actor}`),
);
export const devApply = vi.fn((_n: string) =>
  ok({ done: ["gave Item_A", "sent to X/Y"], held: [] as string[] }),
);
export const goal = (
  name: string,
  met: boolean,
  error: string | null = null,
): Goal => ({
  name,
  want: "true",
  now: error ? null : String(met),
  met,
  error,
});
export const goals: Partial<Record<string, Goal[]>> = {};
/** The one saved setup's name. */
export const setup = { name: "Well" };
export const saved: { expect: string; goals?: string[] } = { expect: "a lens" };
const info = (kind: string, input: DevValue["info"]["input"], more = {}) => ({
  kind,
  input,
  choices: [] as string[],
  default: null as string | null,
  ...more,
});
export const values: Record<string, DevValue> = {
  FakeHill_ArriveEntrance: {
    info: info("Bool", "choice", {
      choices: ["true", "false"],
      default: "false",
    }),
    game: true,
    live: "true",
    expr: null,
  },
  Step_FakeCastle: {
    info: info("Enum", "choice", {
      choices: ["NotReady", "Ready", "Step01", "Complete"],
      default: "NotReady",
    }),
    game: true,
    live: "Step01",
    expr: null,
  },
  "PlayerStatus.CurrentRupee": {
    info: info("Int", "number", { default: "0" }),
    game: true,
    live: "42",
    expr: null,
  },
  FakeFlag_CurrentMap: {
    info: info("String64", "text", { default: "" }),
    game: true,
    live: "FakeHill_Child",
    expr: null,
  },
  Fake_Array: {
    info: info("IntArray", "read"),
    game: true,
    live: "[1, 2]",
    expr: null,
  },
  Flag_A_Exp: {
    info: info("BoolExp", "read"),
    game: true,
    live: null,
    expr: "Step_A == Complete && !Flag_B",
  },
  Step_A: {
    info: info("Enum", "choice", {
      choices: ["NotReady", "Step01", "Complete"],
      default: "NotReady",
    }),
    game: true,
    live: "Step01",
    expr: null,
  },
};
export const devValue = vi.fn((name: string) =>
  values[name] ? ok(values[name]) : err<DevValue>(`no entry named ${name}`),
);
export const devSet = vi.fn((name: string, value: string) =>
  ok(`${name} = ${value}`),
);
export const devStep = vi.fn((quest: string, step: string) =>
  ok(`${quest} moved to ${step}`),
);
export const devGive = vi.fn((item: string, count: number) =>
  ok(`gave ${item} ×${count}`),
);
export const devAge = vi.fn(() => ok("now adult, to FakeHill_Adult/X"));
export const devSwitches = vi.fn((s: DevSwitches) => ok(s));
export const play = vi.fn((_t: string) => ok(null));
/** The picked target's build status. */
export const build = { kind: "current" };
export const setAlwaysOnTop = vi.fn((_on: boolean) => Promise.resolve());

/** What the dev window picks from. */
export const OPTIONS = {
  places: [
    "FakeHill_Child/FakeHill_FrontOfMarket",
    "FakeHill_Adult/FakeHill_FrontOfMarket",
    "FakeKakariko_Child/FakeKakariko_FrontOfGate",
  ],
  spawns: [],
  items: ["Item_A", "Item_FakeBomb"],
  names: ["FakeFlag_IsChild", "Flag_B", ...Object.keys(values)],
  quests: { A: ["NotReady", "Step01", "Complete"] },
};

export const bindings = {
  commands: {
    devState: () =>
      game.state ? ok(game.state) : err("the game isn't running"),
    gameOptions: () => ok(OPTIONS),
    devTeleport: (p: string) => devTeleport(p),
    devActors: (m: string) => devActors(m),
    devWarp: (a: string, s: string) => devWarp(a, s),
    devNearby: () => ok([]),
    devHide: (h: string[]) => ok(h),
    devApply: (n: string) => devApply(n),
    devValue: (n: string) => devValue(n),
    devSet: (n: string, v: string) => devSet(n, v),
    devStep: (q: string, s: string) => devStep(q, s),
    devGive: (i: string, c: number) => devGive(i, c),
    devAge: () => devAge(),
    devSwitches: (s: DevSwitches) => devSwitches(s),
    devGoals: () => ok(goals),
    config: () =>
      ok({
        saved_tests: { [setup.name]: saved },
        targets: [{ name: "FakeNative" }],
      }),
    status: () =>
      ok({ targets: [{ name: "FakeNative", build: { status: build } }] }),
    play: (t: string) => play(t),
  },
};

/** The game running on FakeHill as a child, infinite health on, and the
 * fixtures as they start. */
export function resetGame() {
  game.state = {
    map: "FakeHill_Child",
    child: true,
    loading: false,
    event: false,
    switches: { health: true, magic: false, resources: false },
  };
  for (const name of Object.keys(goals)) delete goals[name];
  setup.name = "Well";
  delete saved.goals;
  build.kind = "current";
}

/** Tauri's window, for the pin. */
export const fakeWindow = {
  getCurrentWindow: () => ({ setAlwaysOnTop }),
};
