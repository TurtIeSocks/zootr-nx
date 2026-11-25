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
import {
  commands,
  type SettingDef,
  type StartInventory,
  type Summary,
} from "@/bindings";
import { keep } from "@/lib/api";
import { type FullConfig, full, type Settings } from "@/lib/config";
import { SettingsView } from "@/settings/settings-view";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { summary, toggleDef } from "@/test/fixtures";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useUi.setPath("settings", { closed: [], reset: "defaults" });
});

const toggle = (id: string, label: string, patch: Partial<SettingDef> = {}) =>
  toggleDef(id, { group: "General", label, ...patch });

const choice = (id: string, label: string, options: string[]): SettingDef => ({
  ...toggle(id, label),
  kind: {
    type: "choice",
    default: options[0]?.toLowerCase() ?? "",
    options: options.map((o) => ({
      id: o.toLowerCase(),
      label: o,
      forces: [],
      starts: [],
      archipelago: { ap: "offered" },
    })),
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

const installed = (settings: Settings) => summary({ settings, chests: 1 });

function show({
  defs = schema,
  settings = {},
  section = "General",
  seed = null,
  older = [],
  devTools = false,
}: {
  defs?: SettingDef[];
  settings?: Settings;
  section?: string;
  seed?: Summary | null;
  /** History's other seeds, newest first. */
  older?: Summary[];
  devTools?: boolean;
} = {}) {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: [] });
  useConfig.setPath("schema", defs);
  useConfig.setPath("config", full({ settings, dev_tools: devTools }));
  useSeeds.setPath("status", { game: true, targets: [] });
  useSeeds.setPath("history", {
    installed: seed,
    entries: [...(seed ? [seed] : []), ...older],
  });
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

test("a choice of three or more is a select", () => {
  const ages = choice("age", "Age", ["Child", "Adult", "Random"]);
  show({ defs: [...schema, ages] });
  const hints = screen.getByRole("combobox", { name: "Gossip stones" });
  expect(hints.textContent).toContain("None");
  expect(screen.queryByRole("group", { name: "Gossip stones" })).toBeNull();
  expect(screen.getByRole("combobox", { name: "Age" })).toBeTruthy();
  expect(screen.queryByRole("group", { name: "Age" })).toBeNull();
});

test("a setting greys out while the toggle it needs is off", () => {
  const shops = toggle("shops", "Shopsanity");
  const slots = {
    ...choice("slots", "Shop slots", ["One", "All"]),
    requires: "shops",
  };
  show({ defs: [shops, slots] });
  const one = () => screen.getByRole("button", { name: "One" });
  expect(one().hasAttribute("disabled")).toBe(true);
  expect(screen.getByText(/Needs/).textContent).toBe(
    "Needs Shopsanity on first",
  );
  cleanup();
  show({ defs: [shops, slots], settings: { shops: true } });
  expect(one().hasAttribute("disabled")).toBe(false);
  expect(screen.queryByText(/Needs/)).toBeNull();
});

test("a toggle whose requirement is unmet shows off, and keeps its value", () => {
  const shops = toggle("shops", "Shopsanity");
  const prices = { ...toggle("prices", "Cheap prices"), requires: "shops" };
  const settings = { prices: true };
  const cheap = () => screen.getByRole("switch", { name: "Cheap prices" });
  show({ defs: [shops, prices], settings });
  expect(cheap().getAttribute("aria-checked")).toBe("false");
  expect(cheap().hasAttribute("disabled")).toBe(true);
  expect(useConfig.getState().config.settings.prices).toBe(true);
  cleanup();
  // With Shopsanity on, the stored value is back.
  show({ defs: [shops, prices], settings: { ...settings, shops: true } });
  expect(cheap().getAttribute("aria-checked")).toBe("true");
  expect(cheap().hasAttribute("disabled")).toBe(false);
});

test("a choice whose requirement is unmet shows no pick, and keeps its value", () => {
  const shops = toggle("shops", "Shopsanity");
  const size = {
    ...choice("size", "Size", ["Small", "Big"]),
    requires: "shops",
  };
  const hints = {
    ...choice("hints", "Gossip stones", [
      "None",
      "Way",
      "Items",
      "Junk",
      "All",
    ]),
    requires: "shops",
  };
  const settings = { size: "big", hints: "items" };
  show({ defs: [shops, size, hints], settings });
  const big = () => screen.getByRole("button", { name: "Big" });
  const select = () => screen.getByRole("combobox");
  expect(big().getAttribute("aria-pressed")).toBe("false");
  expect(select().textContent).toBe("Off");
  expect(useConfig.getState().config.settings).toMatchObject(settings);
  cleanup();
  // With Shopsanity on, the stored picks are back.
  show({ defs: [shops, size, hints], settings: { ...settings, shops: true } });
  expect(big().getAttribute("aria-pressed")).toBe("true");
  expect(select().textContent).toBe("Items");
});

test("an untested description becomes a badge", () => {
  show();
  expect(screen.getByText("Untested")).toBeTruthy();
  expect(screen.getByText("Pop-ups go")).toBeTruthy();
  expect(screen.queryByText(/Untested\./)).toBeNull();
});

test("a setting the AP world leaves out is marked, saying why on focus", async () => {
  show({
    defs: [
      toggle("fake-traps", "Fake traps", {
        archipelago: { ap: "later", why: "the item pool" },
      }),
      toggle("open-forest", "Open forest"),
    ],
  });
  const marks = screen.getAllByText("Not in AP");
  expect(marks).toHaveLength(1);
  fireEvent.focus(marks[0] as HTMLElement);
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    "Not in Archipelago yet (the item pool)",
  );
});

test("a setting the AP world forces on is marked forced, not left out", async () => {
  show({
    defs: [
      toggle("fake-chests", "Fake chests", {
        archipelago: { ap: "on-until", why: "check kinds" },
      }),
    ],
  });
  expect(screen.queryByText("Not in AP")).toBeNull();
  fireEvent.focus(screen.getByText("Forced in AP"));
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    "Always on in Archipelago for now (check kinds)",
  );
});

test("a choice's picks the AP world leaves out are listed under it", () => {
  const keys = choice("fake-keys", "Fake keys", [
    "Vanilla",
    "Rings",
    "Singles",
  ]);
  if (keys.kind.type !== "choice") throw new Error("a choice");
  for (const o of keys.kind.options.slice(1)) {
    o.archipelago = { ap: "later", why: "keys" };
  }
  show({ defs: [keys] });
  expect(screen.getByText("Rings, Singles")).toBeTruthy();
  expect(screen.getAllByText("Not in AP")).toHaveLength(1);
});

test("a checklist item the AP world leaves out is marked", () => {
  const lake = cutscene("lake", "Lake Hylia", null, "By area");
  show({
    defs: [
      cutscene("kokiri", "Kokiri Forest", null, "By area"),
      { ...lake, archipelago: { ap: "never", why: "fake reason" } },
    ],
    section: "Cutscenes",
  });
  const item = screen.getByRole("checkbox", { name: "Lake Hylia" });
  expect(
    within(item.closest("label") as HTMLElement).getByText("Not in AP"),
  ).toBeTruthy();
  expect(screen.getAllByText("Not in AP")).toHaveLength(1);
});

test("a changed setting can be reset, alone or with its group", async () => {
  const onChange = show({
    settings: { "open-forest": true, "chest-size": "all small", other: 1 },
  });
  fireEvent.click(screen.getByRole("button", { name: "Reset Open forest" }));
  expect(saved(onChange)).toEqual({ "chest-size": "all small", other: 1 });
  fireEvent.click(screen.getByRole("button", { name: "Reset to Defaults" }));
  await waitFor(() => expect(saved(onChange)).toEqual({ other: 1 }));
  expect(screen.queryByRole("button", { name: "Reset Skip the intro" })).toBe(
    null,
  );
});

/** Whether the row named `label` shows the change dot. */
const dotted = (label: string) =>
  within(
    screen.getByText(label, { selector: "label" }).parentElement as HTMLElement,
  ).queryByLabelText("changed") !== null;

test("dots mark what differs from the last seed, not the defaults", () => {
  show({
    settings: { "open-forest": true, "chest-size": "all small" },
    seed: installed({ "open-forest": true }),
  });
  expect(dotted("Chest size")).toBe(true);
  expect(dotted("Open forest")).toBe(false);
});

test("the row reset puts the last seed's value back", () => {
  const onChange = show({
    settings: { "chest-size": "all small" },
    seed: installed({ "chest-size": "all small", "open-forest": true }),
  });
  fireEvent.click(screen.getByRole("button", { name: "Reset Open forest" }));
  expect(saved(onChange)).toEqual({
    "chest-size": "all small",
    "open-forest": true,
  });
});

const cutscene = (
  id: string,
  label: string,
  parent: string | null,
  section: string | null = null,
) =>
  toggle(id, label, {
    group: "Cutscenes",
    parent,
    section: section ?? "Cutscenes",
    list: section !== null,
  });

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

test("a list section is a checklist with its count, open until closed", async () => {
  const onChange = show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { kokiri: true },
  });
  const head = screen.getByRole("button", { name: /^By area/ });
  expect(head.getAttribute("aria-expanded")).toBe("true");
  expect(within(head).getByText("1 of 3 on")).toBeTruthy();
  fireEvent.click(head);
  expect(head.getAttribute("aria-expanded")).toBe("false");
  expect(useUi.getState().settings.closed).toEqual(["Cutscenes/By area"]);
  fireEvent.click(head);
  expect(head.getAttribute("aria-expanded")).toBe("true");
  const field = screen.getByRole("checkbox", { name: "Hyrule Field" });
  fireEvent.click(field);
  expect(saved(onChange)).toEqual({ kokiri: true, field: true });
  await waitFor(() => expect(within(head).getByText("2 of 3 on")).toBeTruthy());
  fireEvent.click(screen.getByRole("button", { name: "All in By area" }));
  expect(saved(onChange)).toEqual({ kokiri: true, field: true, lake: true });
});

test("the drift bar counts what differs from the last seed", () => {
  show({ seed: installed({}) });
  expect(screen.queryByText(/since the last seed/)).toBeNull();
  cleanup();
  show({ seed: installed({}), settings: { "open-forest": true } });
  expect(screen.getByText("1 change since the last seed")).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Generate & Install" }),
  ).toBeTruthy();
});

test("with no seed installed, the drift bar counts what differs from the defaults", () => {
  show();
  expect(screen.queryByText(/from the default settings/)).toBeNull();
  cleanup();
  show({ settings: { "open-forest": true } });
  expect(screen.getByText("1 change from the default settings")).toBeTruthy();
});

test("with an AP slot installed, the drift bar counts from the last standard seed", () => {
  // The slot's own settings came with its file and never count.
  const slot = { ...installed({}), id: 2, ap_slot: "FakeSlot" };
  const standard = installed({ "open-forest": true });
  show({ seed: slot, older: [standard], settings: { "open-forest": true } });
  expect(screen.queryByText(/since the last seed/)).toBeNull();
  cleanup();
  show({ seed: slot, older: [standard] });
  expect(screen.getByText(/since the last seed/)).toBeTruthy();
  cleanup();
  show({ seed: slot, settings: { "open-forest": true } });
  expect(screen.getByText(/from the default settings/)).toBeTruthy();
});

test("a checklist item shows the dot when it differs from the last seed", () => {
  show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { kokiri: true, field: true },
    seed: installed({ field: true }),
  });
  const item = (label: string) =>
    within(screen.getByText(label).closest("label") as HTMLElement);
  expect(item("Kokiri Forest").queryByLabelText("changed")).not.toBeNull();
  expect(item("Hyrule Field").queryByLabelText("changed")).toBeNull();
});

/** The header action named `name`. */
const action = (name: string | RegExp) => screen.getByRole("button", { name });

test("Reset to Defaults says why while nothing differs from them", () => {
  const onChange = show();
  const reset = action("Reset to Defaults");
  expect(reset.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(reset);
  expect(onChange).not.toHaveBeenCalled();
});

test("All and None write only what changes, skipping locked toggles", async () => {
  const onChange = show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { [all]: true, kokiri: true },
  });
  // The lake is on because of its parent: left alone.
  fireEvent.click(action("All in By area"));
  await waitFor(() =>
    expect(saved(onChange)).toEqual({ [all]: true, kokiri: true, field: true }),
  );
  cleanup();
  const again = show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { [all]: true, kokiri: true },
  });
  fireEvent.click(action("None in Cutscenes"));
  await waitFor(() =>
    expect(saved(again)).toEqual({ [all]: false, kokiri: true }),
  );
});

test("All says why when every toggle in its section is already on", () => {
  show({
    defs: cutscenes,
    section: "Cutscenes",
    settings: { [all]: true, kokiri: true, field: true },
  });
  expect(action("All in By area").getAttribute("aria-disabled")).toBe("true");
  expect(action("All in Cutscenes").getAttribute("aria-disabled")).toBe("true");
  expect(action("None in Cutscenes").getAttribute("aria-disabled")).toBeNull();
});

test("a section of rows heads itself with its description and count", () => {
  show({ settings: { "open-forest": true } });
  const head = screen.getByRole("button", { name: /^Fake section/ });
  expect(within(head).getByText("Fake about")).toBeTruthy();
  // Open forest on, Skip the intro on by default, Menus open off.
  expect(within(head).getByText("2 of 3 on")).toBeTruthy();
});

test("a section with no toggles counts what's off its defaults", () => {
  show({
    defs: [
      choice("chest-size", "Chest size", ["Vanilla", "Big"]),
      choice("age", "Age", ["Child", "Adult", "Random"]),
    ],
    settings: { age: "adult" },
  });
  const head = screen.getByRole("button", { name: /^Fake section/ });
  expect(within(head).getByText("1 of 2 off default")).toBeTruthy();
});

test("a section of rows turns its toggles all on or all off", async () => {
  const setConfig = show({ settings: { "open-forest": true } });
  fireEvent.click(screen.getByRole("button", { name: "All in Fake section" }));
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({ "open-forest": true, menus: true }),
  );
  fireEvent.click(screen.getByRole("button", { name: "None in Fake section" }));
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({
      "open-forest": false,
      "skip-intro": false,
      menus: false,
    }),
  );
});

test("a section with no toggles has no All or None", () => {
  show({ defs: [choice("chest-size", "Chest size", ["Vanilla", "Big"])] });
  expect(screen.queryByRole("button", { name: /^All/ })).toBeNull();
});

/** Opens the reset's menu and picks `name`. */
async function pickReset(name: RegExp) {
  fireEvent.keyDown(action("More ways to reset"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitem", { name }));
}

test("Reset to Last Seed puts the seed's values back, and leads after", async () => {
  const setConfig = show({
    settings: { "open-forest": true, "chest-size": "all small" },
    seed: installed({ "chest-size": "all small", menus: true }),
  });
  await pickReset(/Reset to Last Seed/);
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({
      "chest-size": "all small",
      menus: true,
    }),
  );
  expect(useUi.getState().settings.reset).toBe("seed");
  expect(action(/^Reset to Last Seed/)).toBeTruthy();
});

test("with no seed built, the defaults lead, and the menu says why", async () => {
  useUi.setPath("settings.reset", "seed");
  show({ settings: { "open-forest": true } });
  expect(action(/^Reset to Defaults/).getAttribute("aria-disabled")).toBeNull();
  fireEvent.keyDown(action("More ways to reset"), { key: "Enter" });
  const item = await screen.findByRole("menuitem", { name: /Last Seed/ });
  expect(item.textContent).toContain("No seed built yet");
});

test("the drift bar shows on every settings page, the same as on a group", () => {
  for (const section of ["settings", "targets", "plando", "test"]) {
    show({ section, devTools: true, settings: { "open-forest": true } });
    expect(screen.getByText("1 change from the default settings")).toBeTruthy();
    cleanup();
  }
});

test("Review lists the changes and puts the base's settings back", async () => {
  const onChange = show({
    seed: installed({ "chest-size": "all small" }),
    settings: { "open-forest": true, "chest-size": "all small" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Review" }));
  fireEvent.click(
    await screen.findByRole("button", { name: /Use the Last Seed's/ }),
  );
  await waitFor(() =>
    expect(saved(onChange)).toEqual({ "chest-size": "all small" }),
  );
});

test("a page's unsaved bar takes the drift bar's place while it shows", () => {
  show({ section: "test", devTools: true, settings: { "open-forest": true } });
  fireEvent.change(screen.getByLabelText("Spawn"), {
    target: { value: "Map/Spot" },
  });
  expect(screen.getByText("Unsaved changes to the test setup")).toBeTruthy();
  expect(screen.queryByText(/1 change from/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  expect(screen.getByText("Unsaved changes to the test setup")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Really discard?" }));
  expect(screen.getByText("1 change from the default settings")).toBeTruthy();
});

test("a closed section is still closed after leaving the page", () => {
  show({ defs: cutscenes, section: "Cutscenes" });
  fireEvent.click(screen.getByRole("button", { name: /^Cutscenes/ }));
  cleanup();
  show({ defs: cutscenes, section: "Cutscenes" });
  const expanded = (name: RegExp) =>
    screen.getByRole("button", { name }).getAttribute("aria-expanded");
  expect(expanded(/^Cutscenes/)).toBe("false");
  expect(expanded(/^By area/)).toBe("true");
});

test("the test setup form keeps its edits until the saved setup changes", () => {
  show({ section: "test", devTools: true });
  const spawn = () => screen.getByLabelText("Spawn") as HTMLInputElement;
  fireEvent.change(spawn(), { target: { value: "Map/Spot" } });
  // A reload that reads the same config changes nothing.
  act(() => useConfig.setPath("config", keep(full({ dev_tools: true }))));
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
  const head = screen.getByRole("button", { name: /^By area/ });
  expect(within(head).getByText("2 of 3 on")).toBeTruthy();
  fireEvent.click(head);
  const lake = screen.getByRole("checkbox", { name: "Lake Hylia" });
  expect(lake.getAttribute("aria-checked")).toBe("true");
  expect(lake.hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "None in By area" }));
  expect(saved(onChange)).toEqual({ [all]: true, kokiri: false });
});

test("a change that fails to save says why", async () => {
  const setConfig = show();
  setConfig.mockResolvedValue({ status: "error", error: "Read-only file" });
  fireEvent.click(screen.getByRole("switch", { name: "Open forest" }));
  expect(await screen.findByText("Read-only file")).toBeTruthy();
});

/** The made-up grid a starting inventory draws: one Hookshot cell. */
const startView = (tier: number) =>
  Promise.resolve({
    status: "ok" as const,
    data: {
      sections: [
        { section: "Inventory" as const, column: 1, width: 6, shown: true },
      ],
      cells: [
        {
          section: "Inventory" as const,
          label: "Hookshot",
          icon: "Hookshot",
          glyph: "unknown" as const,
          tier,
          tiers: 2,
          count: null,
          row: null,
          hidden: false,
        },
      ],
      caps: [null],
      staged: [tier > 0],
      locked: [],
      bars: [],
      status: [],
    },
  });

const inventory: SettingDef = {
  ...toggle("start-inventory", "Starting inventory", { group: "Inventory" }),
  kind: { type: "inventory", default: {} },
};

test("the starting inventory draws the save editor's grid and saves each click", async () => {
  vi.spyOn(commands, "startItems").mockImplementation((s) =>
    startView(s.hookshot ?? 0),
  );
  const click = vi
    .spyOn(commands, "startClick")
    .mockImplementation((s, _at, intent) =>
      Promise.resolve({
        status: "ok" as const,
        data: (intent === "up" && !s.hookshot
          ? { hookshot: 1 }
          : {}) as StartInventory,
      }),
    );
  const setConfig = show({ defs: [inventory], section: "Inventory" });
  const tile = await screen.findByRole("button", { name: /^Hookshot/ });
  fireEvent.click(tile);
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({ "start-inventory": { hookshot: 1 } }),
  );
  expect(click).toHaveBeenLastCalledWith({}, 0, "up", expect.anything());
  await screen.findByRole("button", { name: /Hookshot.*changed/ });
  fireEvent.contextMenu(screen.getByRole("button", { name: /^Hookshot/ }));
  await waitFor(() => expect(saved(setConfig)).toEqual({}));
  expect(click).toHaveBeenLastCalledWith(
    { hookshot: 1 },
    0,
    "down",
    expect.anything(),
  );
});

test("a default starting inventory emptied saves as empty, not unset", async () => {
  vi.spyOn(commands, "startItems").mockImplementation((s) =>
    startView(s.hookshot ?? 0),
  );
  const click = vi.spyOn(commands, "startClick").mockResolvedValue({
    status: "ok",
    data: {},
  });
  const sword = {
    ...inventory,
    kind: { type: "inventory", default: { hookshot: 1 } },
  } as SettingDef;
  const setConfig = show({ defs: [sword], section: "Inventory" });
  fireEvent.contextMenu(
    await screen.findByRole("button", { name: /^Hookshot/ }),
  );
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({ "start-inventory": {} }),
  );
  expect(click).toHaveBeenLastCalledWith(
    { hookshot: 1 },
    0,
    "down",
    expect.anything(),
  );
});

test("an item a setting gives shows locked, naming it, and a click does nothing", async () => {
  const met = toggle("fake-met", "Met Fake", {
    gives: [["on", ["hookshot"]]],
  });
  const load = vi
    .spyOn(commands, "startItems")
    .mockImplementation((_s, given) =>
      startView(given.length).then((v) => ({
        ...v,
        data: { ...v.data, locked: given.length > 0 ? [0] : [null] },
      })),
    );
  const click = vi.spyOn(commands, "startClick");
  const setConfig = show({
    defs: [met, inventory],
    settings: { "fake-met": true },
    section: "Inventory",
  });
  const tile = await screen.findByRole("button", {
    name: "Hookshot, from Met Fake",
  });
  expect(load).toHaveBeenLastCalledWith(
    {},
    [["hookshot"], ["kokiri-tunic"]],
    expect.anything(),
  );
  fireEvent.click(tile);
  fireEvent.contextMenu(tile);
  expect(click).not.toHaveBeenCalled();
  expect(setConfig).not.toHaveBeenCalled();
});

test("the Kokiri Tunic shows held and locked to every new game", async () => {
  const load = vi
    .spyOn(commands, "startItems")
    .mockImplementation((_s, given) =>
      startView(1).then((v) => ({
        ...v,
        data: {
          ...v.data,
          cells: v.data.cells.map((c) => ({ ...c, label: "Kokiri Tunic" })),
          locked: [given.findIndex((w) => w.includes("kokiri-tunic"))],
        },
      })),
    );
  show({ defs: [inventory], section: "Inventory" });
  await screen.findByRole("button", {
    name: "Kokiri Tunic, from every new game",
  });
  expect(load).toHaveBeenLastCalledWith(
    {},
    [["kokiri-tunic"]],
    expect.anything(),
  );
});

test("a cell the settings keep out shows locked, saying why", async () => {
  vi.spyOn(commands, "startItems").mockImplementation((_s, given) =>
    startView(0).then((v) => ({
      ...v,
      data: { ...v.data, locked: [given.length], bars: ["Needs Fake Moved"] },
    })),
  );
  const click = vi.spyOn(commands, "startClick");
  show({ defs: [inventory], section: "Inventory" });
  const tile = await screen.findByRole("button", {
    name: "Hookshot, needs Fake Moved",
  });
  fireEvent.click(tile);
  expect(click).not.toHaveBeenCalled();
});

test("All starts with the arsenal and None with nothing", async () => {
  vi.spyOn(commands, "startItems").mockImplementation(() => startView(0));
  const arsenal = vi
    .spyOn(commands, "startArsenal")
    .mockImplementation((s) =>
      Promise.resolve({ status: "ok", data: { ...s, longshot: 1 } }),
    );
  const setConfig = show({
    defs: [inventory],
    settings: { "start-inventory": { hookshot: 1 } },
    section: "Inventory",
  });
  await screen.findByRole("button", { name: /^Hookshot/ });
  fireEvent.click(
    screen.getByRole("button", { name: "All in Starting inventory" }),
  );
  await waitFor(() =>
    expect(saved(setConfig)).toEqual({
      "start-inventory": { hookshot: 1, longshot: 1 },
    }),
  );
  expect(arsenal).toHaveBeenCalledWith({ hookshot: 1 }, expect.anything());
  fireEvent.click(
    screen.getByRole("button", { name: "None in Starting inventory" }),
  );
  await waitFor(() => expect(saved(setConfig)).toEqual({}));
});

test("turning developer tools off on Test setup leaves for the first group", () => {
  show({ section: "test", devTools: true });
  expect(screen.getByLabelText("Spawn")).toBeTruthy();
  act(() => useConfig.setPath("config.dev_tools", false));
  expect(useUi.getState().section).toBe("General");
  expect(screen.queryByLabelText("Spawn")).toBeNull();
});

/** A door Adult forces on. */
const door = toggle("door", "Door");
const age: SettingDef = {
  ...choice("age", "Age", ["Child", "Adult"]),
  kind: {
    type: "choice",
    default: "child",
    options: [
      {
        id: "child",
        label: "Child",
        forces: [],
        starts: [],
        archipelago: { ap: "offered" },
      },
      {
        id: "adult",
        label: "Adult",
        forces: ["door"],
        starts: [],
        archipelago: { ap: "offered" },
      },
    ],
  },
};

test("a choice that forces names what it affects, and each name jumps to its row", () => {
  show({ defs: [age, door] });
  // Child forces nothing, yet the caption names what Adult would.
  expect(screen.getByText("Affects")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Door" }));
  const row = document.getElementById("setting-door")?.closest("[data-reveal]");
  expect(row?.classList.contains("animate-flash")).toBe(true);
});

test("a forced row's reason jumps to the setting forcing it", () => {
  show({ defs: [age, door], settings: { age: "adult" } });
  expect(screen.getByText(/On because of/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Age" }));
  const row = document.getElementById("setting-age")?.closest("[data-reveal]");
  expect(row?.classList.contains("animate-flash")).toBe(true);
});
