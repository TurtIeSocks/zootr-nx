import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { act, Profiler } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { commands, events, type Summary } from "@/bindings";
import { type FullTest, full } from "@/lib/config";
import { PlayView } from "@/play/play-view";
import { stop } from "@/stores/ap";
import { useConfig } from "@/stores/config";
import { refresh, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

const picked = vi.fn(async () => "/seeds/AP_1_P1_Link.apznx" as string | null);
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => picked(),
  save: vi.fn(),
}));

afterEach(() => {
  cleanup();
  stop();
  vi.restoreAllMocks();
  clearMocks();
});

const target = { name: "Emu", app: "/a.app", mod_dir: "/m" };
const seed = {
  id: 1,
  time: 0,
  seed: "42",
  settings: {},
  label: {},
  chests: 104,
  changed: 92,
  icons: ["Sun", "Moon", "Sun", "Key", "Fish"],
  share: "ZR1-shared",
  test: false,
  plando: false,
  ap_slot: null,
  death_link: false,
};

const ok = { status: "ok", data: null } as const;

/** Reads back what the backend holds after a write. */
function backend(history: { installed: Summary | null; entries: Summary[] }) {
  vi.spyOn(commands, "status").mockResolvedValue({
    status: "ok",
    data: { game: true, targets: [] },
  });
  return vi
    .spyOn(commands, "history")
    .mockResolvedValue({ status: "ok", data: history });
}

function show(
  installed: Summary | null,
  settings = {},
  test: Partial<FullTest> = {},
  older: Summary[] = [],
  devTools = false,
) {
  useConfig.setPath("schema", [
    {
      id: "x",
      group: "g",
      label: "X",
      description: "",
      kind: { type: "toggle", default: false },
      parent: null,
      requires: null,
      section: "Fake section",
      section_about: "Fake about",
      list: false,
      gives: [],
      archipelago: { ap: "offered" },
    },
  ]);
  useConfig.setPath(
    "config",
    full({ targets: [target], settings, test, dev_tools: devTools }),
  );
  useSeeds.setPath("status", { game: true, targets: [] });
  useSeeds.setPath("history", {
    installed,
    entries: [...(installed ? [installed] : []), ...older],
  });
  useUi.setPath("play", { tab: null, history: "all" });
  // An AP slot's page listens to the client.
  vi.spyOn(events.apEvent, "listen").mockResolvedValue(() => {});
  vi.spyOn(commands, "apState").mockResolvedValue({
    status: "offline",
    log: [],
  });
  render(<PlayView />);
}

/** Opens the folder under the tab whose name starts with `name`. */
const openTab = (name: string) =>
  fireEvent.mouseDown(
    screen.getByRole("tab", { name: new RegExp(`^${name}`) }),
  );

const button = (name: string | RegExp) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

/** The card or folder under the heading, such as "Installed" or "New
 * seed". */
const card = (heading: string) =>
  within(
    screen
      .getByRole("heading", { name: heading })
      .closest("section, [role=tabpanel]") ?? document.body,
  );

test("with nothing installed the game plays vanilla", () => {
  show(null);
  expect(screen.getByText("Vanilla")).toBeTruthy();
  expect(button("Play Emu").disabled).toBe(false);
  expect(screen.queryByRole("button", { name: "Uninstall" })).toBeNull();
});

test("the Installed card leaves Spoiler Log, copy and edit to History", () => {
  show(seed);
  openTab("History");
  const installed = card("Installed");
  expect(installed.queryByRole("button", { name: "Spoiler Log" })).toBeNull();
  expect(
    installed.queryByRole("button", { name: "Copy share code" }),
  ).toBeNull();
  expect(installed.queryByRole("button", { name: "Label seed" })).toBeNull();
  expect(installed.queryByRole("button", { name: "Uninstall" })).toBeNull();
  const history = card("History");
  expect(history.getByRole("button", { name: "Spoiler Log" })).toBeTruthy();
  expect(history.getByRole("button", { name: "Copy share code" })).toBeTruthy();
  expect(history.getByRole("button", { name: "Label seed" })).toBeTruthy();
});

test("the Uninstall button lives in New seed", () => {
  show(seed);
  expect(
    card("New Seed").getByRole("button", { name: "Uninstall" }),
  ).toBeTruthy();
});

test("an installed seed can be taken off for a vanilla run", async () => {
  const uninstall = vi.spyOn(commands, "uninstall").mockResolvedValue(ok);
  backend({ installed: null, entries: [seed] });
  show(seed);
  fireEvent.click(card("New Seed").getByRole("button", { name: "Uninstall" }));
  await waitFor(() =>
    expect(card("Installed").getByText("Vanilla")).toBeTruthy(),
  );
  expect(uninstall).toHaveBeenCalledOnce();
});

test("the overview marks settings new since the last seed", () => {
  show(seed);
  expect(screen.getByText("Same as the last seed")).toBeTruthy();
  expect(screen.getByText("Defaults")).toBeTruthy();
  cleanup();
  show(seed, { x: true });
  expect(screen.getByText("1 change since the last seed")).toBeTruthy();
  const tile = button(/^g/);
  expect(within(tile).getByText("1 on")).toBeTruthy();
  expect(within(tile).getByText("X").parentElement?.dataset.mark).toBe("new");
});

test("a setting the installed seed had and the next won't is struck", () => {
  show({ ...seed, settings: { x: true } });
  const chip = within(button(/^g/)).getByText("X").parentElement;
  expect(chip?.dataset.mark).toBe("gone");
  expect(within(button(/^g/)).queryByText(/ on$/)).toBeNull();
});

test("with nothing installed the overview counts from the defaults", () => {
  show(null, { x: true });
  expect(screen.getByText("1 change from the default settings")).toBeTruthy();
});

test("Review lists the differences and puts the installed settings back", async () => {
  const set = vi.spyOn(commands, "setConfig").mockResolvedValue({
    status: "ok",
    data: [],
  });
  backend({ installed: seed, entries: [seed] });
  show({ ...seed, label: { name: "FakeRun" } }, { x: true });
  fireEvent.click(button("Review"));
  expect(await screen.findByText("Changed since FakeRun")).toBeTruthy();
  expect(screen.getByText("off")).toBeTruthy();
  expect(screen.getByText("on")).toBeTruthy();
  fireEvent.click(button("Use the Last Seed's Settings"));
  await waitFor(() => expect(set).toHaveBeenCalled());
  expect(set.mock.calls[0]?.[0].settings).toEqual({});
});

test("a group tile opens its settings page", () => {
  show(null);
  fireEvent.click(button(/^g/));
  expect(useUi.getState()).toMatchObject({ tab: "Settings", section: "g" });
});

test("a share code shows its own settings and leaves the person's alone", async () => {
  const read = vi.spyOn(commands, "shareSettings").mockResolvedValue({
    status: "ok",
    data: { settings: { x: true }, pins: 0, test: {} },
  });
  show(seed);
  act(() => useUi.setPath("newSeed.seed", "ZR1-FakeCode"));
  expect(await screen.findByText("The Code Builds With")).toBeTruthy();
  expect(read).toHaveBeenCalledWith("ZR1-FakeCode");
  expect(screen.queryByText(/Same as/)).toBeNull();
  const chip = within(button(/^g/)).getByText("X").parentElement;
  expect(chip?.dataset.mark).toBeUndefined();
  // Plando only: Test setup hides with developer tools off.
  expect(screen.getAllByText("Not in this code")).toHaveLength(1);
  fireEvent.click(button("Clear Code"));
  expect(useUi.getState().newSeed.seed).toBe("");
  expect(screen.getByText("Same as the last seed")).toBeTruthy();
});

test("an installed seed can be played", () => {
  show(seed);
  openTab("History");
  expect(
    screen.getAllByRole("button", { name: "Copy share code" }),
  ).toHaveLength(1);
  expect(screen.queryByText("42")).toBeNull();
  expect(button("Play Emu").disabled).toBe(false);
});

test("a seed shows its icons and can't delete itself while installed", () => {
  show(seed);
  openTab("History");
  expect(
    screen.getAllByRole("img", { name: "Sun, Moon, Sun, Key, Fish" }).length,
  ).toBe(2);
  expect(button("Delete seed").getAttribute("aria-disabled")).toBe("true");
  expect(button("Installed").getAttribute("aria-disabled")).toBe("true");
  expect(button("Use settings").getAttribute("aria-disabled")).toBe("true");
  expect(screen.queryByRole("button", { name: "Install" })).toBeNull();
});

test("a seed's name and note show in history and prefill its label", async () => {
  show({ ...seed, label: { name: "Saria's Hookshot", note: "Q31\nQ32" } });
  openTab("History");
  expect(screen.getAllByText("Saria's Hookshot")).toHaveLength(2);
  const [label] = screen.getAllByRole("button", { name: "Label seed" });
  fireEvent.click(label as HTMLElement);
  expect(
    ((await screen.findByLabelText("Name")) as HTMLInputElement).value,
  ).toBe("Saria's Hookshot");
  expect((screen.getByLabelText("Note") as HTMLTextAreaElement).value).toBe(
    "Q31\nQ32",
  );
});

test("a seed's settings can be used once they differ from the current ones", () => {
  show(seed, { x: true });
  openTab("History");
  expect(button("Use settings").getAttribute("aria-disabled")).toBeNull();
});

test("a seed built with a test setup carries a badge", () => {
  show({ ...seed, test: true });
  openTab("History");
  // The installed card and its history row.
  expect(screen.getAllByText("Test")).toHaveLength(2);
  cleanup();
  show(seed);
  expect(screen.queryByText("Test")).toBeNull();
});

test("without developer tools the overview has no Test setup tile", () => {
  show(null, {}, { spawn: "Map/Spot" });
  expect(screen.queryByRole("button", { name: /^Test Setup/ })).toBeNull();
  expect(button(/^Plando/)).toBeTruthy();
});

test("a stored test setup shows in the overview and opens from there", () => {
  show(null, {}, {}, [], true);
  expect(within(button(/^Test Setup/)).getByText("Off")).toBeTruthy();
  cleanup();
  show(null, {}, { spawn: "Map/Spot", give: ["Item_A"] }, [], true);
  const tile = within(button(/^Test Setup/));
  expect(tile.getByText("starts at Map/Spot")).toBeTruthy();
  expect(tile.getByText("1 item")).toBeTruthy();
  fireEvent.click(button(/^Test Setup/));
  expect(useUi.getState()).toMatchObject({ tab: "Settings", section: "test" });
});

test("a plando seed carries its own badge beside a test one", () => {
  show({ ...seed, plando: true, test: true });
  openTab("History");
  expect(screen.getAllByText("Plando")).toHaveLength(2);
  expect(screen.getAllByText("Test")).toHaveLength(2);
});

test("the plando draft shows in the overview and opens from there", () => {
  show(null);
  act(() =>
    useConfig.setPath("config", {
      ...useConfig.getState().config,
      place: { 1: "Item_B", 2: "Item_C" },
      plandos: { Hooks: { 2: "Item_C", 1: "Item_B" }, Idle: {} },
    }),
  );
  const tile = within(button(/^Plando/));
  expect(tile.getByText("2 pinned")).toBeTruthy();
  expect(tile.getByText("Hooks")).toBeTruthy();
  expect(tile.getByText("2 checks pinned")).toBeTruthy();
  fireEvent.click(button(/^Plando/));
  expect(useUi.getState()).toMatchObject({
    tab: "Settings",
    section: "plando",
  });
});

test("deleting a seed and clearing history each ask for a second click", async () => {
  const del = vi.spyOn(commands, "deleteSeed").mockResolvedValue(ok);
  backend({
    installed: null,
    entries: [
      { ...seed, id: 3 },
      { ...seed, id: 4 },
    ],
  });
  show(null, {}, {}, [
    { ...seed, id: 2 },
    { ...seed, id: 3 },
  ]);
  openTab("History");
  const [first] = screen.getAllByRole("button", { name: "Delete seed" });
  fireEvent.click(first as HTMLElement);
  expect(del).not.toHaveBeenCalled();
  expect((first as HTMLElement).textContent).toBe("Really delete?");
  fireEvent.click(first as HTMLElement);
  await waitFor(() => expect(del).toHaveBeenCalledWith(2));
  fireEvent.click(button("Clear"));
  expect(button("Really clear?")).toBeTruthy();
});

test("a pasted share code goes to Generate whole, a seed as digits", async () => {
  // Its steps come back on a channel, which needs the IPC.
  mockIPC(() => {});
  const generate = vi.spyOn(commands, "generate").mockResolvedValue({
    status: "ok",
    data: seed,
  });
  backend({ installed: seed, entries: [seed] });
  show(null);
  const box = screen.getByRole("textbox", { name: "Seed or share code" });
  fireEvent.change(box, { target: { value: " ZR1-ab_c-9 " } });
  fireEvent.click(button("Generate & Install"));
  await waitFor(() =>
    expect(generate).toHaveBeenCalledWith(
      "ZR1-ab_c-9",
      true,
      expect.anything(),
    ),
  );
  fireEvent.change(box, { target: { value: "12a34" } });
  expect((box as HTMLInputElement).value).toBe("1234");
});

test("the dice button is named and rolls a seed into the input", () => {
  show(null);
  const box = screen.getByRole("textbox", {
    name: "Seed or share code",
  }) as HTMLInputElement;
  expect(box.value).toBe("");
  fireEvent.click(button("Roll a seed"));
  expect(box.value).toMatch(/^\d+$/);
});

test("an AP seed installs from the .apznx picked, and a cancel does nothing", async () => {
  mockIPC(() => {});
  const generateAp = vi.spyOn(commands, "generateAp").mockResolvedValue({
    status: "ok",
    data: seed,
  });
  backend({ installed: seed, entries: [seed] });
  show(null);
  openTab("Archipelago");
  fireEvent.click(button("Browse…"));
  await waitFor(() =>
    expect(generateAp).toHaveBeenCalledWith(
      "/seeds/AP_1_P1_Link.apznx",
      expect.anything(),
    ),
  );
  picked.mockResolvedValueOnce(null);
  fireEvent.click(button("Browse…"));
  await waitFor(() => expect(picked).toHaveBeenCalledTimes(2));
  expect(generateAp).toHaveBeenCalledTimes(1);
});

test("the seed typed into New seed is still there after leaving the tab", () => {
  show(null);
  fireEvent.change(
    screen.getByRole("textbox", { name: "Seed or share code" }),
    {
      target: { value: "1234" },
    },
  );
  cleanup();
  show(null);
  expect(
    (
      screen.getByRole("textbox", {
        name: "Seed or share code",
      }) as HTMLInputElement
    ).value,
  ).toBe("1234");
});

test("a history poll doesn't clobber a label being typed", async () => {
  show({ ...seed, label: { name: "Old" } });
  openTab("History");
  fireEvent.click(button("Label seed"));
  const name = (await screen.findByLabelText("Name")) as HTMLInputElement;
  fireEvent.change(name, { target: { value: "New name" } });
  useSeeds.setPath("history", {
    installed: { ...seed, label: { name: "Old" } },
    entries: [
      { ...seed, label: { name: "Old" } },
      { ...seed, id: 9 },
    ],
  });
  await waitFor(() => expect(screen.getAllByText("Unnamed seed")).toBeTruthy());
  expect(name.value).toBe("New name");
});

test("a poll that finds nothing new renders nothing", async () => {
  const installed = { ...seed, label: { name: "Old" } };
  backend({ installed, entries: [installed] });
  show(installed);
  const commits = vi.fn();
  cleanup();
  render(
    <Profiler id="play" onRender={commits}>
      <PlayView />
    </Profiler>,
  );
  commits.mockClear();
  await act(refresh);
  expect(commands.history).toHaveBeenCalled();
  expect(commits).not.toHaveBeenCalled();
});

test("Generate Only adds a seed to History while the game is open", async () => {
  mockIPC(() => {});
  const generate = vi.spyOn(commands, "generate").mockResolvedValue({
    status: "ok",
    data: seed,
  });
  backend({ installed: seed, entries: [seed] });
  show(null);
  act(() => useSeeds.setPath("blocked", "Close the game first"));
  expect(button("Generate & Install").getAttribute("aria-disabled")).toBe(
    "true",
  );
  fireEvent.keyDown(button("More ways to generate"), { key: "Enter" });
  fireEvent.click(
    await screen.findByRole("menuitem", { name: /Generate Only/ }),
  );
  await waitFor(() =>
    expect(generate).toHaveBeenCalledWith(null, false, expect.anything()),
  );
});

test("a blocked Generate stays hoverable, says why, and generates nothing", () => {
  const generate = vi.spyOn(commands, "generate");
  show(null);
  act(() => useSeeds.setPath("blocked", "Add a Target first"));
  const go = button("Generate & Install");
  expect(go.getAttribute("aria-disabled")).toBe("true");
  expect(go.disabled).toBe(false);
  fireEvent.click(go);
  expect(generate).not.toHaveBeenCalled();
  expect(card("New Seed").getByText("Add a Target first")).toBeTruthy();
});

test("Standard, Archipelago and History each open their folder", () => {
  show(seed);
  expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual([
    "Standard",
    "Archipelago",
    "History1",
  ]);
  expect(screen.getByRole("heading", { name: "New Seed" })).toBeTruthy();
  openTab("History");
  expect(screen.getByRole("heading", { name: "History" })).toBeTruthy();
  expect(screen.queryByRole("heading", { name: "New Seed" })).toBeNull();
  expect(useUi.getState().play.tab).toBe("history");
});

test("the folder last open opens again, else the installed seed's kind", () => {
  show(seed);
  openTab("History");
  cleanup();
  render(<PlayView />);
  expect(screen.getByRole("tab", { selected: true }).textContent).toMatch(
    /^History/,
  );
  cleanup();
  show({ ...seed, ap_slot: "FakeSlot" });
  expect(screen.getByRole("tab", { selected: true }).textContent).toBe(
    "Archipelago",
  );
});

test("arrow keys move between the tabs", async () => {
  show(seed);
  const selected = () => screen.getByRole("tab", { selected: true });
  const standard = screen.getByRole("tab", { name: "Standard" });
  act(() => standard.focus());
  fireEvent.keyDown(standard, { key: "ArrowRight" });
  await waitFor(() => expect(selected().textContent).toBe("Archipelago"));
  fireEvent.keyDown(selected(), { key: "End" });
  await waitFor(() => expect(selected().textContent).toMatch(/^History/));
});

test("an installed AP slot is named by its slot, badged, and shows its room", () => {
  show({ ...seed, ap_slot: "FakeSlot" });
  openTab("Standard");
  const hero = card("Installed");
  expect(hero.getByText("FakeSlot")).toBeTruthy();
  expect(hero.getByText("Archipelago")).toBeTruthy();
  expect(hero.getByRole("status").textContent).toContain("No room yet");
  fireEvent.click(hero.getByRole("button", { name: "Set Up Room" }));
  expect(screen.getByRole("tab", { selected: true }).textContent).toBe(
    "Archipelago",
  );
});

test("a standard seed's hero has no room line", () => {
  show(seed);
  expect(card("Installed").queryByRole("status")).toBeNull();
});

test("History filters by kind, and a slot's row can't reuse or share its settings", () => {
  const slot = { ...seed, id: 2, ap_slot: "FakeSlot" };
  show(seed, {}, {}, [slot]);
  openTab("History");
  const history = card("History");
  const names = () =>
    history.getAllByRole("listitem").map((li) => li.textContent ?? "");
  expect(names()).toHaveLength(2);
  const [first, row] = history.getAllByRole("listitem") as [
    HTMLElement,
    HTMLElement,
  ];
  expect(row.textContent).toContain("FakeSlot");
  expect(row.textContent).toContain("Archipelago");
  const actions = (li: HTMLElement) =>
    within(li)
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label") ?? b.textContent);
  expect(actions(row)).not.toContain("Use settings");
  expect(actions(row)).not.toContain("Copy share code");
  expect(actions(first)).toContain("Use settings");
  fireEvent.click(history.getByRole("button", { name: "Archipelago" }));
  expect(names()).toEqual([expect.stringContaining("FakeSlot")]);
  expect(useUi.getState().play.history).toBe("archipelago");
  fireEvent.click(history.getByRole("button", { name: "Standard" }));
  expect(names()).toHaveLength(1);
  expect(names()[0]).not.toContain("FakeSlot");
});
