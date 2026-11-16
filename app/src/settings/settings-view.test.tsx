import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands, type SettingDef, type Summary } from "@/bindings";
import { keep } from "@/lib/api";
import { type FullConfig, full, type Settings } from "@/lib/config";
import { SettingsView } from "@/settings/settings-view";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const toggle = (
  id: string,
  label: string,
  patch: Partial<SettingDef> = {},
): SettingDef => ({
  id,
  group: "General",
  label,
  description: "",
  kind: { type: "toggle", default: false },
  parent: null,
  subgroup: null,
  ...patch,
});

const choice = (id: string, label: string, options: string[]): SettingDef => ({
  ...toggle(id, label),
  kind: {
    type: "choice",
    default: options[0]?.toLowerCase() ?? "",
    options: options.map((o) => ({ id: o.toLowerCase(), label: o })),
  },
});

const schema: SettingDef[] = [
  toggle("open-forest", "Open forest", { description: "Mido moves" }),
  toggle("skip-intro", "Skip the intro", {
    kind: { type: "toggle", default: true },
  }),
  choice("chest-size", "Chest size", ["Vanilla", "All small"]),
  choice("hints", "Gossip stones", ["None", "Way", "Items", "Junk", "All"]),
  toggle("menus", "Menus open", { description: "Untested. Pop-ups go" }),
  toggle("elsewhere", "Not in this group", { group: "Cutscenes" }),
];

const installed = (settings: Settings): Summary => ({
  id: 1,
  time: 0,
  seed: "42",
  icons: [],
  share: "",
  settings,
  label: {},
  chests: 1,
  changed: 0,
  test: false,
  plando: false,
});

function show({
  defs = schema,
  settings = {},
  section = "General",
  seed = null,
}: {
  defs?: SettingDef[];
  settings?: Settings;
  section?: string;
  seed?: Summary | null;
} = {}) {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: null });
  useConfig.setPath("schema", defs);
  useConfig.setPath("config", full({ settings }));
  useSeeds.setPath("status", { game: true, targets: [] });
  useSeeds.setPath("history", { installed: seed, entries: [] });
  useUi.setPath("section", section);
  render(<SettingsView />);
  return setConfig;
}

/** The settings last saved. */
const saved = (setConfig: ReturnType<typeof show>) =>
  (setConfig.mock.lastCall?.[0] as FullConfig | undefined)?.settings;

test("a toggle renders a switch at its value", () => {
  const onChange = show();
  const forest = screen.getByRole("switch", { name: "Open forest" });
  expect(forest.getAttribute("aria-checked")).toBe("false");
  expect(
    screen
      .getByRole("switch", { name: "Skip the intro" })
      .getAttribute("aria-checked"),
  ).toBe("true");
  expect(screen.queryByText("Not in this group")).toBeNull();
  fireEvent.click(forest);
  expect(saved(onChange)).toEqual({ "open-forest": true });
});

test("a two-option choice is a segmented control", () => {
  const onChange = show();
  const size = screen.getByRole("group", { name: "Chest size" });
  const small = within(size).getByRole("button", { name: "All small" });
  expect(
    within(size)
      .getByRole("button", { name: "Vanilla" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  fireEvent.click(small);
  expect(saved(onChange)).toEqual({ "chest-size": "all small" });
});

test("a five-option choice is a select", () => {
  show();
  const hints = screen.getByRole("combobox", { name: "Gossip stones" });
  expect(hints.textContent).toContain("None");
  expect(screen.queryByRole("group", { name: "Gossip stones" })).toBeNull();
});

test("an untested description becomes a badge", () => {
  show();
  expect(screen.getByText("Untested")).toBeTruthy();
  expect(screen.getByText("Pop-ups go")).toBeTruthy();
  expect(screen.queryByText(/Untested\./)).toBeNull();
});

test("a changed setting can be reset, alone or with its group", async () => {
  const onChange = show({
    settings: { "open-forest": true, "chest-size": "all small", other: 1 },
  });
  fireEvent.click(screen.getByRole("button", { name: "Reset Open forest" }));
  expect(saved(onChange)).toEqual({ "chest-size": "all small", other: 1 });
  fireEvent.click(
    await screen.findByRole("button", { name: "Reset 1 change" }),
  );
  expect(saved(onChange)).toEqual({ other: 1 });
  expect(screen.queryByRole("button", { name: "Reset Skip the intro" })).toBe(
    null,
  );
});

const cutscene = (
  id: string,
  label: string,
  parent: string | null,
  subgroup: string | null = null,
) => toggle(id, label, { group: "Cutscenes", parent, subgroup });

const all = "skip-cutscenes";
const cutscenes = [
  cutscene(all, "Skip all cutscenes", null),
  cutscene("skip-owl", "Skip Kaepora Gaebora", all),
  cutscene("kokiri", "Kokiri Forest", null, "By area"),
  cutscene("field", "Hyrule Field", null, "By area"),
  cutscene("lake", "Lake Hylia", all, "By area"),
];

test("a child of an on parent is locked, with the reason", () => {
  show({ defs: cutscenes, section: "Cutscenes", settings: { [all]: true } });
  const owl = screen.getByRole("switch", { name: "Skip Kaepora Gaebora" });
  expect(owl.getAttribute("aria-checked")).toBe("true");
  expect(owl.hasAttribute("disabled")).toBe(true);
  expect(screen.getAllByText("Skip all cutscenes").length).toBeGreaterThan(1);
  expect(screen.getAllByText(/On because of/).length).toBeGreaterThan(0);
  expect(
    screen
      .getByRole("switch", { name: "Skip all cutscenes" })
      .hasAttribute("disabled"),
  ).toBe(false);
});

test("a subgroup is a closed checklist with its count", async () => {
  const onChange = show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { kokiri: true },
  });
  const head = screen.getByRole("button", { name: /By area/ });
  expect(head.getAttribute("aria-expanded")).toBe("false");
  expect(within(head).getByText("1 of 3 on")).toBeTruthy();
  fireEvent.click(head);
  expect(head.getAttribute("aria-expanded")).toBe("true");
  const field = screen.getByRole("checkbox", { name: "Hyrule Field" });
  fireEvent.click(field);
  expect(saved(onChange)).toEqual({ kokiri: true, field: true });
  await waitFor(() => expect(within(head).getByText("2 of 3 on")).toBeTruthy());
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  expect(saved(onChange)).toEqual({ kokiri: true, field: true, lake: true });
});

test("the drift bar counts what differs from the installed seed", () => {
  show({ seed: installed({}) });
  expect(screen.queryByText(/from the installed seed/)).toBeNull();
  cleanup();
  show({ seed: installed({}), settings: { "open-forest": true } });
  expect(screen.getByText(/from the installed seed/)).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Generate & install" }),
  ).toBeTruthy();
});

test("with no seed installed, the drift bar counts what differs from the defaults", () => {
  show();
  expect(screen.queryByText(/from the default settings/)).toBeNull();
  cleanup();
  show({ settings: { "open-forest": true } });
  expect(screen.getByText(/from the default settings/)).toBeTruthy();
});

test("an open checklist is still open after leaving the page", () => {
  show({ defs: cutscenes, section: "Cutscenes" });
  fireEvent.click(screen.getByRole("button", { name: /By area/ }));
  cleanup();
  show({ defs: cutscenes, section: "Cutscenes" });
  expect(
    screen
      .getByRole("button", { name: /By area/ })
      .getAttribute("aria-expanded"),
  ).toBe("true");
});

test("the test setup form keeps its edits until the saved setup changes", () => {
  show({ section: "test" });
  const spawn = () => screen.getByLabelText("Spawn") as HTMLInputElement;
  fireEvent.change(spawn(), { target: { value: "Map/Spot" } });
  // A reload that reads the same config changes nothing.
  act(() => useConfig.setPath("config", keep(full({}))));
  expect(spawn().value).toBe("Map/Spot");
  act(() =>
    useConfig.setPath("config.test", (t) => ({ ...t, expect: "a lens" })),
  );
  expect(spawn().value).toBe("");
});

test("a locked checklist item stays put, and None leaves it on", () => {
  const onChange = show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { [all]: true, kokiri: true },
  });
  const head = screen.getByRole("button", { name: /By area/ });
  expect(within(head).getByText("2 of 3 on")).toBeTruthy();
  fireEvent.click(head);
  const lake = screen.getByRole("checkbox", { name: "Lake Hylia" });
  expect(lake.getAttribute("aria-checked")).toBe("true");
  expect(lake.hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "None" }));
  expect(saved(onChange)).toEqual({ [all]: true, kokiri: false, field: false });
});

test("a change that fails to save says why", async () => {
  const setConfig = show();
  setConfig.mockResolvedValue({ status: "error", error: "Read-only file" });
  fireEvent.click(screen.getByRole("switch", { name: "Open forest" }));
  expect(await screen.findByText("Read-only file")).toBeTruthy();
});
