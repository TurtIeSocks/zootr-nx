import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { GameOptions } from "@/bindings";
import { type FullConfig, full, fullTest, NO_TEST } from "@/lib/config";
import { TestPane } from "@/settings/test-pane";
import { useConfig } from "@/stores/config";

const setConfig = vi.fn((_c: FullConfig) =>
  Promise.resolve({ status: "ok", data: [] }),
);

/** A scan that found nothing. */
const NONE: GameOptions = {
  places: [],
  spawns: [],
  items: [],
  names: [],
  quests: {},
};

/** Answers the option scan by hand instead, when set. */
const scans: { answer?: () => Promise<GameOptions> } = {};

vi.mock("@/bindings", () => ({
  commands: {
    gameOptions: async () => ({
      status: "ok",
      data: scans.answer
        ? await scans.answer()
        : {
            places: [],
            spawns: ["FakeGrave_Child/Spot_A"],
            items: ["Item_A", "Item_B"],
            names: ["IsGet.Item_B"],
            quests: { FakeSage: ["Step01", "Step02"] },
          },
    }),
    devValue: async () => ({
      status: "ok",
      data: {
        info: {
          kind: "Bool",
          input: "choice",
          choices: ["true", "false"],
          default: "false",
        },
        game: false,
        live: null,
      },
    }),
    // Two placements of one actor, as the backend lists them.
    devActors: async () => ({
      status: "ok",
      data: [
        { actor: "Npc_A", hash: "1", label: null },
        { actor: "Npc_A", hash: "2", label: "Npc_A_2" },
      ],
    }),
    shuffleCommand: () =>
      Promise.resolve({ status: "ok", data: "zootr shuffle --give Item_B" }),
    setConfig: (c: FullConfig) => setConfig(c),
  },
}));

afterEach(() => {
  cleanup();
  setConfig.mockClear();
  scans.answer = undefined;
});

/** What was last saved is what the store now holds, so a later save builds
 * on it. */
function show(config: FullConfig = full({})) {
  useConfig.setPath("config", config);
  render(<TestPane />);
  return setConfig;
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;
const type = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
/** Types a whole `NAME=VALUE` row and adds it. */
const add = (label: string, noun: string, line: string) => {
  type(label, line);
  fireEvent.click(button(`Add ${noun}`));
};

test("saves a spawn, known items, flags and goals as the test setup", async () => {
  const onChange = show();
  await waitFor(() =>
    expect(screen.getByPlaceholderText("an item's name")).toBeTruthy(),
  );
  expect(screen.queryByRole("button", { name: "Save Setup" })).toBeNull();
  type("Spawn", "FakeGrave_Child/Spot_A");
  type("Near", " Npc_A ");
  fireEvent.click(button("Back"));
  type("Items", "Item_C");
  expect(screen.getByText("Not a pouch item")).toBeTruthy();
  expect(button("Add item").disabled).toBe(true);
  type("Items", "Item_A");
  fireEvent.click(button("Add item"));
  add("New-Game Flags", "flag", "A=1");
  add("New-Game Flags", "flag", " B=2 ");
  add("Quest Steps", "quest step", "FakeSage=Step02");
  add("Quest Steps", "quest step", "FakeCastle=Step02");
  add("Goals", "goal", "C=true");
  type("Expect", " a lens ");
  fireEvent.click(button("Save Setup"));
  expect(onChange).toHaveBeenCalledWith({
    ...full({}),
    test: {
      spawn: "FakeGrave_Child/Spot_A",
      give: ["Item_A"],
      defaults: ["A=1", "B=2"],
      progress: ["FakeSage=Step02", "FakeCastle=Step02"],
      near: "Npc_A",
      side: "back",
      hour: null,
      goals: ["C=true"],
      expect: "a lens",
    },
  });
});

test("the bar floats only while the form differs, and Discard puts the saved setup back", async () => {
  show(full({ test: { give: ["Item_B"] } }));
  expect(screen.queryByText("Unsaved changes to the test setup")).toBeNull();
  type("Spawn", "FakeGrave_Child/Spot_A");
  expect(screen.getByText("Unsaved changes to the test setup")).toBeTruthy();
  type("Spawn", "");
  expect(screen.queryByText("Unsaved changes to the test setup")).toBeNull();
  fireEvent.click(button("Remove Item_B"));
  fireEvent.click(button("Discard"));
  expect(screen.queryByText("Item_B")).toBeNull();
  fireEvent.click(button("Really discard?"));
  expect(screen.getByText("Item_B")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Discard" })).toBeNull();
});

test("the bar goes once the setup is saved", async () => {
  show();
  type("Spawn", "FakeGrave_Child/Spot_A");
  fireEvent.click(button("Save Setup"));
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Save Setup" })).toBeNull(),
  );
});

test("quest steps alone make a setup that clears and saves", async () => {
  const config = full({ test: { progress: ["FakeSage=Step02"] } });
  const onChange = show(config);
  fireEvent.click(button("Remove FakeSage=Step02"));
  add("Quest Steps", "quest step", "FakeCastle=Step02");
  fireEvent.click(button("Save Setup"));
  expect(onChange).toHaveBeenLastCalledWith({
    ...config,
    test: { ...config.test, progress: ["FakeCastle=Step02"] },
  });
  await waitFor(() => expect(button("Clear").disabled).toBe(false));
  fireEvent.click(button("Clear"));
  expect(onChange).not.toHaveBeenLastCalledWith({ ...config, test: NO_TEST });
  fireEvent.click(button("Really clear?"));
  expect(onChange).toHaveBeenLastCalledWith({ ...config, test: NO_TEST });
  expect(screen.queryByText("FakeCastle=Step02")).toBeNull();
});

test("a known quest offers its steps, starting at a new game's", async () => {
  show();
  await waitFor(() =>
    expect(screen.getByPlaceholderText("an item's name")).toBeTruthy(),
  );
  type("Quest Steps", "FakeSage");
  const select = () => screen.getByLabelText("Quest Steps value");
  await waitFor(() => expect(select().tagName).toBe("SELECT"));
  const step = select() as HTMLSelectElement;
  // Pinned like the text field it replaces, so the name keeps the row.
  expect(step.className).toContain("w-40");
  await waitFor(() => expect(step.value).toBe("Step01"));
  fireEvent.change(step, { target: { value: "Step02" } });
  fireEvent.click(button("Add quest step"));
  expect(screen.getByText("FakeSage=Step02")).toBeTruthy();
  type("Quest Steps", "NoSuchQuest");
  expect(screen.getByText(/no quest by that name/)).toBeTruthy();
});

test("near suggests the spawn map's actors", async () => {
  show();
  type("Spawn", "FakeGrave_Child/Spot_A");
  type("Near", "c_A");
  const list = await screen.findByLabelText("Near suggestions");
  expect(list.textContent).toBe("Npc_A");
});

test("warns on an unknown spawn", async () => {
  show();
  await waitFor(() =>
    expect(screen.getByPlaceholderText("an item's name")).toBeTruthy(),
  );
  type("Spawn", "Nowhere/AtAll");
  expect(screen.getByText(/never sends Link there/)).toBeTruthy();
});

test("clear empties a stored setup and removing an item drops it", async () => {
  const config = full({
    test: { spawn: null, give: ["Item_B", "Item_A"], defaults: [] },
  });
  const onChange = show(config);
  fireEvent.click(button("Remove Item_B"));
  expect(screen.queryByText("Item_B")).toBeNull();
  fireEvent.click(button("Clear"));
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.click(button("Really clear?"));
  expect(onChange).toHaveBeenCalledWith({
    ...config,
    test: NO_TEST,
  });
});

test("saves a setup by name, switches to one and deletes one", async () => {
  const well = fullTest({
    spawn: "Cellar/FakeCellar_SideA",
    give: ["Item_A"],
  });
  const config = full({ saved_tests: { Well: well } });
  const onChange = show(config);
  expect(button("Save As").disabled).toBe(true);
  fireEvent.click(button("Well"));
  expect(onChange).toHaveBeenLastCalledWith({ ...config, test: well });
  type("Spawn", "FakeGrave_Child/Spot_A");
  type("Saved Setups", " Grave ");
  await waitFor(() => expect(button("Save As").disabled).toBe(false));
  fireEvent.click(button("Save As"));
  const grave = fullTest({ spawn: "FakeGrave_Child/Spot_A" });
  await waitFor(() =>
    expect(onChange).toHaveBeenLastCalledWith({
      ...config,
      test: grave,
      saved_tests: { Well: well, Grave: grave },
    }),
  );
  await waitFor(() => expect(button("Delete Well").disabled).toBe(false));
  fireEvent.click(button("Delete Well"));
  expect(button("Delete Well").textContent).toBe("Really delete?");
  expect(onChange).toHaveBeenLastCalledWith({
    ...config,
    test: grave,
    saved_tests: { Well: well, Grave: grave },
  });
  fireEvent.click(button("Delete Well"));
  await waitFor(() =>
    expect(onChange).toHaveBeenLastCalledWith({
      ...config,
      test: grave,
      saved_tests: { Grave: grave },
    }),
  );
});

test("copies the shuffle command once the setup is saved", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  show(full({ test: { give: ["Item_B"] } }));
  type("Spawn", "FakeGrave_Child/Spot_A");
  expect(button("Copy as CLI").disabled).toBe(true);
  type("Spawn", "");
  fireEvent.click(button("Copy as CLI"));
  await waitFor(() => expect(button("Copied")).toBeTruthy());
  expect(writeText).toHaveBeenCalledWith("zootr shuffle --give Item_B");
});

test("a slow option scan for a game no longer set is dropped", async () => {
  const answers: ((o: GameOptions) => void)[] = [];
  scans.answer = () => new Promise((resolve) => answers.push(resolve));
  show(full({ game: "/dumps/old" }));
  await waitFor(() => expect(answers).toHaveLength(1));
  act(() => useConfig.setPath("config.game", "/dumps/new"));
  await waitFor(() => expect(answers).toHaveLength(2));
  await act(async () => answers[1]?.({ ...NONE, items: ["Item_D"] }));
  await act(async () => answers[0]?.({ ...NONE, items: ["Item_E"] }));
  type("Items", "Item_");
  const items = screen.getByLabelText("Items suggestions");
  expect(items.textContent).toBe("Item_D");
});
