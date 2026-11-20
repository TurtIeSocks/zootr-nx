import type { Config, Status, TestSetup } from "@/bindings";
import { count, LISTED } from "@/lib/format";

/** A test setup with every field present. */
export type FullTest = Required<TestSetup>;
/** Config with every field present. The binding marks them optional because
 * they default when the file is read. */
export interface FullConfig extends Required<Omit<Config, "test">> {
  test: FullTest;
}
/** The setting values by id, as the config keeps them. */
export type Settings = FullConfig["settings"];

/** A test setup that changes nothing. */
export const NO_TEST: FullTest = {
  spawn: null,
  give: [],
  defaults: [],
  progress: [],
  near: null,
  side: null,
  hour: null,
  goals: [],
  expect: "",
};

/** A test setup with its missing fields filled from {@link NO_TEST}. */
export const fullTest = (t?: TestSetup): FullTest => ({ ...NO_TEST, ...t });

/** A config with its missing fields filled with their defaults. */
export const full = (c: Config): FullConfig => ({
  game: null,
  settings: {},
  targets: [],
  picked: null,
  dev_tools: false,
  dev_panel: false,
  game_assets: false,
  no_update_check: false,
  ...c,
  test: fullTest(c.test),
  saved_tests: c.saved_tests ?? {},
  place: c.place ?? {},
  plandos: c.plandos ?? {},
});

/** Whether a test setup changes anything. */
export const testOn = (t: FullTest) =>
  t.spawn !== null ||
  t.give.length > 0 ||
  t.defaults.length > 0 ||
  t.progress.length > 0;

/** A test setup in a few words, a phrase per part: where it starts, its
 * items, flags and quest steps. `items` writes the items themselves, or
 * how many. */
export function testParts(
  t: FullTest,
  items = (give: string[]) =>
    give.length > LISTED ? count(give.length, "item") : give.join(", "),
): string[] {
  return [
    t.spawn && `starts at ${t.spawn}`,
    t.give.length > 0 && items(t.give),
    t.defaults.length > 0 && count(t.defaults.length, "flag"),
    t.progress.length > 0 && count(t.progress.length, "quest step"),
  ].filter((p): p is string => !!p);
}

/** Why no seed can be built right now, or "" when one can. Building one
 * for History alone needs neither a target nor the game closed. */
export function unbuildable(status: Status): string {
  return status.game ? "" : "Set the game dump in Settings first";
}

/** Why nothing can be installed right now, or "" when something can. */
export function blocked(status: Status, config: FullConfig): string {
  if (status.targets.some((t) => t.running)) return "Close the game first";
  if (!status.game) return unbuildable(status);
  if (config.targets.length === 0) return "Add a target in Settings first";
  return "";
}
