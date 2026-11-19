import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "@/app";
import {
  type Config,
  commands,
  type SettingDef,
  type Summary,
} from "@/bindings";
import { Sidebar } from "@/shell/sidebar";
import { useChecks } from "@/stores/checks";
import { useConfig } from "@/stores/config";
import { useLive } from "@/stores/live";
import { useSeeds } from "@/stores/seeds";
import { openSpoiler, useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";

const toggle = (id: string, group: string): SettingDef => ({
  id,
  group,
  label: id,
  description: "",
  kind: { type: "toggle", default: false },
  parent: null,
  subgroup: null,
});

const seed: Summary = {
  id: 7,
  time: 0,
  seed: "42",
  settings: {},
  label: {},
  chests: 104,
  changed: 92,
  icons: ["Sun", "Moon", "Sun", "Key", "Fish"],
  test: false,
  plando: false,
  ap_slot: null,
  death_link: false,
  share: "",
};

const backend = {
  config: {
    targets: [{ name: "Emu", app: "/a.app", mod_dir: "/m" }],
  } as Config,
  installed: null as Summary | null,
  running: false,
};

vi.mock("@/bindings", () => ({
  events: {
    liveEvent: { listen: () => Promise.resolve(() => {}) },
    checksEvent: { listen: () => Promise.resolve(() => {}) },
    itemsEvent: { listen: () => Promise.resolve(() => {}) },
  },
  commands: {
    schema: () =>
      Promise.resolve([toggle("a", "General"), toggle("b", "Cutscenes")]),
    config: () => Promise.resolve({ status: "ok", data: backend.config }),
    status: () =>
      Promise.resolve({
        status: "ok",
        data: {
          game: true,
          targets: [
            { name: "Emu", app: true, mod_dir: true, running: backend.running },
          ],
        },
      }),
    history: () =>
      Promise.resolve({
        status: "ok",
        data: {
          installed: backend.installed,
          entries: backend.installed ? [backend.installed] : [],
        },
      }),
    spoiler: vi.fn(() =>
      Promise.resolve({
        status: "ok",
        data: { spots: [], events: [], unprovable: [] },
      }),
    ),
    liveState: () =>
      Promise.resolve({
        status: { state: "Waiting", reason: null },
        events: [],
        dropped: 0,
        next: 0,
      }),
    checksState: () =>
      Promise.resolve({ status: "ok", data: { id: null, rows: [] } }),
    itemsState: () =>
      Promise.resolve({
        status: "ok",
        data: { cells: [], options: { sections: [] } },
      }),
    itemsIcons: () => Promise.resolve({ status: "ok", data: null }),
  },
}));

afterEach(() => {
  cleanup();
  backend.installed = null;
  backend.running = false;
  useUi.setPath("tab", "Play");
});

/** The sidebar over stores holding two groups, the first changed. */
function sidebar() {
  useConfig.setPath("groups", [
    { name: "General", changed: true },
    { name: "Cutscenes", changed: false },
  ]);
  useSeeds.setPath("status", { game: true, targets: [] });
  render(<Sidebar />);
}

const item = (name: RegExp) => screen.getByRole("button", { name });

test("the Live item carries a dot while recording or waiting", () => {
  useSeeds.setPath("history", { installed: seed, entries: [seed] });
  useLive.setPath("status.state", "Waiting");
  sidebar();
  expect(screen.getByLabelText("waiting")).toBeTruthy();
  act(() => useLive.setPath("status.state", "Recording"));
  const live = item(/^Live/);
  expect(live.contains(screen.getByLabelText("recording"))).toBe(true);
  fireEvent.click(live);
  expect(useUi.getState().tab).toBe("Live");
});

/** The run group's buttons, found by its heading. */
const runItems = (heading: string) =>
  within(screen.getByText(heading).parentElement as HTMLElement)
    .getAllByRole("button")
    .map((b) => b.textContent);

test("Play sits alone above the run group, which holds the seed's views", () => {
  useSeeds.setPath("history", { installed: seed, entries: [seed] });
  sidebar();
  const nav = screen.getByRole("navigation");
  expect(nav.querySelector("button")?.textContent).toBe("Play");
  expect(runItems("Current run")).toEqual([
    "Checks",
    "Items",
    "Live",
    "Spoiler",
  ]);
});

test("the run group is the current run whether or not the game runs", () => {
  useSeeds.setPath("history", { installed: seed, entries: [seed] });
  sidebar();
  expect(runItems("Current run")).toHaveLength(4);
  act(() => useSeeds.setPath("running", true));
  expect(runItems("Current run")).toHaveLength(4);
});

test("the run group is hidden with no seed installed", () => {
  sidebar();
  expect(screen.queryByText("Current run")).toBeNull();
  for (const name of [/^Checks/, /^Items/, /^Live/, /^Spoiler/])
    expect(screen.queryByRole("button", { name })).toBeNull();
  act(() => useSeeds.setPath("history", { installed: seed, entries: [seed] }));
  expect(runItems("Current run")).toHaveLength(4);
});

test("the Spoiler item opens the Spoiler tab", () => {
  useSeeds.setPath("history", { installed: seed, entries: [seed] });
  sidebar();
  fireEvent.click(item(/^Spoiler/));
  expect(useUi.getState().tab).toBe("Spoiler");
  expect(item(/^Spoiler/).getAttribute("aria-current")).toBe("page");
  expect(useShell.getState().spoiler).toBeUndefined();
});

test("the Spoiler tab shows the installed seed's spoiler inline once revealed", async () => {
  backend.installed = seed;
  render(<App />);
  const nav = await screen.findByRole("navigation");
  fireEvent.click(within(nav).getByRole("button", { name: /^Spoiler/ }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Reveal spoiler" }),
  );
  expect(await screen.findByText(/only its checks show/)).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(commands.spoiler).toHaveBeenCalledWith(7);
});

test("History's spoiler of the installed seed is covered too", async () => {
  vi.mocked(commands.spoiler).mockClear();
  backend.installed = seed;
  render(<App />);
  await screen.findByRole("navigation");
  act(() => openSpoiler(seed));
  const dialog = await screen.findByRole("dialog");
  expect(
    within(dialog).getByRole("button", { name: "Reveal spoiler" }),
  ).toBeTruthy();
  expect(commands.spoiler).not.toHaveBeenCalled();
});

test("while the game runs, the Spoiler tab stays covered until revealed", async () => {
  vi.mocked(commands.spoiler).mockClear();
  backend.installed = seed;
  backend.running = true;
  useUi.setPath("tab", "Spoiler");
  render(<App />);
  const reveal = await screen.findByRole("button", { name: "Reveal spoiler" });
  expect(commands.spoiler).not.toHaveBeenCalled();
  fireEvent.click(reveal);
  expect(await screen.findByText(/only its checks show/)).toBeTruthy();
  // It stays revealed for this seed when the tab opens again.
  act(() => useUi.setPath("tab", "Checks"));
  act(() => useUi.setPath("tab", "Spoiler"));
  expect(screen.queryByRole("button", { name: "Reveal spoiler" })).toBeNull();
});

test("the Spoiler tab says so with no seed installed", async () => {
  useUi.setPath("tab", "Spoiler");
  render(<App />);
  expect(await screen.findByText("No seed is installed")).toBeTruthy();
});

test("each schema group is an item, marked when it holds changes", () => {
  useSeeds.setPath("history", { installed: seed, entries: [seed] });
  useChecks.setState((c) => ({ ...c, id: 7, done: 41, total: 112 }));
  useConfig.setPath("config.test.spawn", "Map/Spot");
  sidebar();
  const nav = screen.getByRole("navigation");
  expect(within(item(/^Checks/)).getByText("41/112")).toBeTruthy();
  expect(within(item(/^General/)).getByLabelText("changed")).toBeTruthy();
  expect(within(item(/^Cutscenes/)).queryByLabelText("changed")).toBeNull();
  expect(within(item(/^Test setup/)).getByText("On")).toBeTruthy();
  fireEvent.click(within(nav).getByRole("button", { name: /^Cutscenes/ }));
  expect(useUi.getState()).toMatchObject({
    tab: "Settings",
    section: "Cutscenes",
  });
});

test("the dev panel button is greyed with the setting off, never disabled", () => {
  sidebar();
  const wrench = item(/^Dev panel/);
  expect(wrench.classList.contains("opacity-45")).toBe(true);
  expect(wrench.getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(wrench);
  expect(useShell.getState().beckon).toBeGreaterThan(0);
  act(() => useConfig.setPath("config.dev_panel", true));
  expect(wrench.classList.contains("opacity-45")).toBe(false);
});

test("with the dev panel off, its button opens Settings at the toggle", async () => {
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Dev panel" }));
  expect(
    await screen.findByRole("heading", { level: 1, name: "Settings" }),
  ).toBeTruthy();
  const toggle = screen.getByRole("switch", { name: "Dev panel" });
  expect(toggle.closest(".animate-beckon")).toBeTruthy();
});

test("the current page is marked", () => {
  useUi.setPath("tab", "Settings");
  useUi.setPath("section", "Cutscenes");
  sidebar();
  expect(item(/^Cutscenes/).getAttribute("aria-current")).toBe("page");
  expect(item(/^Play/).getAttribute("aria-current")).toBeNull();
});

test("clicking a group in the app opens its page", async () => {
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /^Cutscenes/ }));
  expect(
    await screen.findByRole("heading", { level: 1, name: "Cutscenes" }),
  ).toBeTruthy();
  expect(item(/^Cutscenes/).getAttribute("aria-current")).toBe("page");
});

test("with no targets the app opens on Targets", async () => {
  backend.config = {};
  render(<App />);
  expect(
    await screen.findByRole("heading", { level: 1, name: "Targets" }),
  ).toBeTruthy();
});
