import { afterEach, expect, test, vi } from "vitest";
import type { Row, SettingDef } from "@/bindings";
import { GROUP_HELP, TOPICS } from "@/help/topics";
import { APP_FIELDS } from "@/settings/app-fields";
import {
  type ActionState,
  actions,
  checks,
  type Hit,
  pages,
  recents,
  search,
  seeds,
  settings,
} from "@/shell/find/sources";
import { useUi } from "@/stores/ui";
import { summary, toggleDef } from "@/test/fixtures";

vi.mock("@/shell/find/reveal", async (real) => ({
  ...(await real<typeof import("@/shell/find/reveal")>()),
  reveal: vi.fn(),
}));

// Restored even when an assertion fails, so fake timers never leak on.
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

const def = (over: Partial<SettingDef> = {}) =>
  toggleDef("fake-cows", {
    group: "Shuffles",
    label: "Shuffle fake cows",
    description: "Untested. Each fake cow gives an item.",
    section_about: "",
    ...over,
  });

const row = (hash: string, label: string | null = null): Row => ({
  spot: {
    hash,
    area: "FakeArea",
    area_name: "Fake Forest",
    holder: "",
    vanilla: "FakeChest",
    item: "",
    place: `Fake Place ${hash}`,
  },
  checked: false,
  tracked: false,
  flag: null,
  label,
  mark: null,
});

const STATE: ActionState = {
  blocked: "",
  unbuildable: "",
  generating: false,
  busy: false,
  picked: "Emu",
  cantPlay: "",
  stale: false,
  theme: "system",
  generate: vi.fn(),
  stop: vi.fn(),
  play: vi.fn(() => Promise.resolve(undefined)),
  nextTheme: vi.fn(),
  checkNow: vi.fn(() => Promise.resolve("Up to date")),
};

const titled = (hits: Hit[], title: string) =>
  hits.find((h) => h.title === title);
const said = (id: keyof typeof TOPICS) => TOPICS[id].body.join(" ");

test("pages carry their trail, and the topics on them as their line", () => {
  const hits = pages(["Shuffles"], false, true);
  expect(titled(hits, "Targets")).toMatchObject({
    trail: ["App"],
    body: said("target"),
  });
  // Variables sits on Targets too: matched on, not shown.
  expect(titled(hits, "Targets")?.words).toContain(said("variables"));
  expect(titled(hits, "History")).toMatchObject({
    trail: ["Play"],
    body: said("history"),
  });
  expect(titled(hits, "Shuffles")).toMatchObject({
    trail: ["Randomizer"],
    body: GROUP_HELP.Shuffles?.body.join(" "),
  });
  expect(titled(hits, "Save Editor")?.trail).toEqual([]);
});

test("pages the sidebar hides stay hidden", () => {
  const titles = (devTools: boolean, installed: boolean) =>
    pages([], devTools, installed).map((h) => h.title);
  expect(titles(false, true)).not.toContain("Test Setup");
  expect(titles(true, true)).toContain("Test Setup");
  expect(titles(false, false)).not.toContain("Checks");
  expect(titles(false, true)).toEqual(
    expect.arrayContaining(["Checks", "Items", "Live", "Spoiler Log"]),
  );
});

test("settings: the schema's by group, the app's and the targets' fields", () => {
  const hits = settings([def()], false);
  expect(titled(hits, "Shuffle fake cows")).toMatchObject({
    trail: ["Randomizer", "Shuffles"],
    body: "Each fake cow gives an item.",
  });
  expect(titled(hits, "Mod Folder")).toMatchObject({
    trail: ["App", "Targets"],
    body: said("mod-folder"),
  });
  expect(titled(hits, APP_FIELDS.game.label)).toMatchObject({
    trail: ["App", "Settings"],
    body: said("game-dump"),
  });
  expect(titled(hits, APP_FIELDS.devTools.label)?.body).toBe(
    APP_FIELDS.devTools.description,
  );
  // A field only developers see.
  const dev = (on: boolean) =>
    settings([], on).some((h) => h.title.startsWith("Control File"));
  expect([dev(false), dev(true)]).toEqual([false, true]);
});

test("opening a setting opens its page and its closed section", () => {
  // revealSetting calls the real reveal, which retries on a timer for its
  // control. Left running, a retry outlived the file's jsdom in CI.
  vi.useFakeTimers();
  useUi.setPath("settings.closed", ["Shuffles/Fake section", "Other/x"]);
  const [cows] = settings([def()], false);
  cows?.run();
  expect(useUi.getPath("tab")).toBe("Settings");
  expect(useUi.getPath("section")).toBe("Shuffles");
  expect(useUi.getPath("settings.closed")).toEqual(["Other/x"]);
});

test("a blocked action greys out and says why; Stop shows only while generating", () => {
  const hits = actions({ ...STATE, blocked: "Choose your game dump first" });
  expect(titled(hits, "Generate & Install")).toMatchObject({
    muted: true,
    body: "Choose your game dump first",
  });
  expect(titled(hits, "Generate Only")?.muted).toBe(false);
  expect(titled(hits, "Stop Generating")).toBeUndefined();
  const busy = actions({ ...STATE, generating: true });
  expect(titled(busy, "Generate Only")?.body).toBe("A seed is generating");
  expect(titled(busy, "Stop Generating")).toBeTruthy();
});

test("Play names its target, and a stale game opens Play to ask", () => {
  const play = titled(actions(STATE), "Play Emu");
  expect(play?.run()).toBeInstanceOf(Promise);
  const stale = titled(actions({ ...STATE, stale: true }), "Play Emu");
  expect(stale?.run()).toBeUndefined();
  expect(useUi.getPath("tab")).toBe("Play");
  expect(useUi.getPath("play.tab")).toBe("standard");
});

test("seeds by name and note, checks by place and label", () => {
  const [named, bare] = seeds([
    summary({
      id: 142,
      label: { name: "Fake forest test", note: "open the fake gate" },
    }),
    summary({ id: 9, seed: "77" }),
  ]);
  expect(named).toMatchObject({
    trail: ["History"],
    title: "Fake forest test",
    body: "#142 · open the fake gate",
  });
  expect(bare).toMatchObject({ title: "Seed 77", body: "#9" });
  const [plain, labelled] = checks([row("a"), row("b", "fake skull")]);
  expect(plain).toMatchObject({
    trail: ["Checks", "Fake Forest"],
    title: "Fake Place a",
    body: undefined,
  });
  expect(labelled).toMatchObject({
    title: "Fake Place b",
    body: "your label: fake skull",
  });
});

test("search wants every word in any order, title hits first", () => {
  const hits = settings([], true);
  expect(search(hits, "folder mod")[0]?.title).toBe("Mod Folder");
  expect(search(hits, "")).toEqual([]);
  // The label matches on its line, the other in its title, which wins.
  const byBody = search(
    checks([row("x", "fake moss"), row("fake moss")]),
    "moss",
  );
  expect(byBody.map((h) => h.title)).toEqual([
    "Fake Place fake moss",
    "Fake Place x",
  ]);
});

test("recents come back newest first, and a gone one drops", () => {
  const hits = seeds([summary({ id: 1 }), summary({ id: 2 })]);
  const back = recents(hits, [
    { group: "seeds", key: "2" },
    { group: "seeds", key: "404" },
    { group: "seeds", key: "1" },
  ]);
  expect(back.map((h) => [h.group, h.found.key])).toEqual([
    ["recent", "2"],
    ["recent", "1"],
  ]);
});
