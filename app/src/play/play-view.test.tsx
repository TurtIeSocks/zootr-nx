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
import { commands, type Summary } from "@/bindings";
import { type FullTest, full } from "@/lib/config";
import { PlayView } from "@/play/play-view";
import { useConfig } from "@/stores/config";
import { refresh, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
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
) {
  useConfig.setPath("schema", [
    {
      id: "x",
      group: "g",
      label: "X",
      description: "",
      kind: { type: "toggle", default: false },
      parent: null,
      subgroup: null,
    },
  ]);
  useConfig.setPath("config", full({ targets: [target], settings, test }));
  useSeeds.setPath("status", { game: true, targets: [] });
  useSeeds.setPath("history", {
    installed,
    entries: [...(installed ? [installed] : []), ...older],
  });
  render(<PlayView />);
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

/** The card under the heading, such as "Installed" or "New seed". */
const card = (heading: string) =>
  within(
    screen.getByRole("heading", { name: heading }).closest("section") ??
      document.body,
  );

test("with nothing installed the game plays vanilla", () => {
  show(null);
  expect(screen.getByText("Vanilla")).toBeTruthy();
  expect(button("Play Emu").disabled).toBe(false);
  expect(screen.queryByRole("button", { name: "Vanilla" })).toBeNull();
});

test("the Installed card leaves Spoiler, copy and edit to History", () => {
  show(seed);
  const installed = card("Installed");
  expect(installed.queryByRole("button", { name: "Spoiler" })).toBeNull();
  expect(
    installed.queryByRole("button", { name: "Copy share code" }),
  ).toBeNull();
  expect(installed.queryByRole("button", { name: "Label seed" })).toBeNull();
  expect(installed.queryByRole("button", { name: "Vanilla" })).toBeNull();
  const history = card("History");
  expect(history.getByRole("button", { name: "Spoiler" })).toBeTruthy();
  expect(history.getByRole("button", { name: "Copy share code" })).toBeTruthy();
  expect(history.getByRole("button", { name: "Label seed" })).toBeTruthy();
});

test("the vanilla button lives in New seed", () => {
  show(seed);
  expect(
    card("New seed").getByRole("button", { name: "Vanilla" }),
  ).toBeTruthy();
});

test("an installed seed can be taken off for a vanilla run", async () => {
  const uninstall = vi.spyOn(commands, "uninstall").mockResolvedValue(ok);
  backend({ installed: null, entries: [seed] });
  show(seed);
  fireEvent.click(card("New seed").getByRole("button", { name: "Vanilla" }));
  await waitFor(() =>
    expect(card("Installed").getByText("Vanilla")).toBeTruthy(),
  );
  expect(uninstall).toHaveBeenCalledOnce();
});

test("settings changed since the installed seed are flagged", () => {
  show(seed);
  expect(screen.queryByText("Not installed yet")).toBeNull();
  cleanup();
  show(seed, { x: true });
  expect(screen.getByText("Not installed yet")).toBeTruthy();
});

test("an installed seed can be played", () => {
  show(seed);
  expect(
    screen.getAllByRole("button", { name: "Copy share code" }),
  ).toHaveLength(1);
  expect(screen.queryByText("42")).toBeNull();
  expect(button("Play Emu").disabled).toBe(false);
});

test("a seed shows its icons and can't delete itself while installed", () => {
  show(seed);
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
  expect(button("Use settings").getAttribute("aria-disabled")).toBeNull();
});

test("a seed built with a test setup carries a badge", () => {
  show({ ...seed, test: true });
  // The installed card and its history row.
  expect(screen.getAllByText("test")).toHaveLength(2);
  cleanup();
  show(seed);
  expect(screen.queryByText("test")).toBeNull();
});

test("a stored test setup shows by Generate and edits from there", () => {
  show(null);
  expect(screen.queryByText(/starts at/)).toBeNull();
  cleanup();
  show(null, {}, { spawn: "Map/Spot", give: ["Item_A"] });
  expect(screen.getByText("starts at Map/Spot · Item_A")).toBeTruthy();
  fireEvent.click(button("Edit test setup"));
  expect(useUi.getState()).toMatchObject({ tab: "Settings", section: "test" });
});

test("a plando seed carries its own badge beside a test one", () => {
  show({ ...seed, plando: true, test: true });
  expect(screen.getAllByText("plando")).toHaveLength(2);
  expect(screen.getAllByText("test")).toHaveLength(2);
});

test("the plando draft shows by Generate and edits from there", () => {
  show(null);
  act(() =>
    useConfig.setPath("config", {
      ...useConfig.getState().config,
      place: { 1: "Item_Hookshot", 2: "Item_Bomb" },
      plandos: { Hooks: { 2: "Item_Bomb", 1: "Item_Hookshot" }, Idle: {} },
    }),
  );
  expect(screen.getByText("Plando Hooks: 2 checks pinned")).toBeTruthy();
  fireEvent.click(button("Edit plando"));
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
  const [first] = screen.getAllByRole("button", { name: "Delete seed" });
  fireEvent.click(first as HTMLElement);
  expect(del).not.toHaveBeenCalled();
  fireEvent.click(first as HTMLElement);
  await waitFor(() => expect(del).toHaveBeenCalledWith(2));
  fireEvent.click(button("Clear"));
  expect(button("Really clear?")).toBeTruthy();
});

test("a pasted share code goes to Generate whole, a seed as digits", async () => {
  const generate = vi.spyOn(commands, "generate").mockResolvedValue({
    status: "ok",
    data: seed,
  });
  backend({ installed: seed, entries: [seed] });
  show(null);
  const box = screen.getByRole("textbox", { name: "Seed or share code" });
  fireEvent.change(box, { target: { value: " ZR1-ab_c-9 " } });
  fireEvent.click(button("Generate & install"));
  await waitFor(() => expect(generate).toHaveBeenCalledWith("ZR1-ab_c-9"));
  fireEvent.change(box, { target: { value: "12a34" } });
  expect((box as HTMLInputElement).value).toBe("1234");
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

test("a blocked Generate stays hoverable, says why, and generates nothing", () => {
  const generate = vi.spyOn(commands, "generate");
  show(null);
  act(() => useSeeds.setPath("blocked", "Add a target first"));
  const go = button("Generate & install");
  expect(go.getAttribute("aria-disabled")).toBe("true");
  expect(go.disabled).toBe(false);
  fireEvent.click(go);
  expect(generate).not.toHaveBeenCalled();
  expect(card("New seed").getByText("Add a target first")).toBeTruthy();
});
