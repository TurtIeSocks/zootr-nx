import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { DevState, DevSwitches, DevValue, Goal } from "@/bindings";
import { DevWindow } from "@/dev/dev-window";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: string };
const ok = <T,>(data: T): Promise<Result<T>> =>
  Promise.resolve({ status: "ok", data });
const err = <T,>(error: string): Promise<Result<T>> =>
  Promise.resolve({ status: "error", error });

const game = {
  state: {
    map: "FakeHill_Child",
    child: true,
    loading: false,
    event: false,
    switches: { health: false, magic: false, resources: false },
  } as DevState | undefined,
};

const devTeleport = vi.fn((_p: string) => ok("sent"));
const placed = (actor: string, hash: string, label: string | null = null) => ({
  actor,
  hash,
  label,
});
const devActors = vi.fn((_map: string) =>
  ok([
    placed("Npc_A", "1"),
    placed("Obj_A", "2", "Field_000"),
    placed("Obj_A", "3"),
  ]),
);
const devWarp = vi.fn((actor: string, _side: string) =>
  ok(`warped beside ${actor}`),
);
const devApply = vi.fn((_n: string) =>
  ok({ done: ["gave Item_A", "sent to X/Y"], held: [] as string[] }),
);
const goal = (
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
const goals: Partial<Record<string, Goal[]>> = {};
/** The one saved setup's name. */
const setup = { name: "Well" };
const saved: { expect: string; goals?: string[] } = { expect: "a lens" };
const info = (kind: string, input: DevValue["info"]["input"], more = {}) => ({
  kind,
  input,
  choices: [] as string[],
  default: null as string | null,
  ...more,
});
const values: Record<string, DevValue> = {
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
const devValue = vi.fn((name: string) =>
  values[name] ? ok(values[name]) : err<DevValue>(`no entry named ${name}`),
);
const devSet = vi.fn((name: string, value: string) => ok(`${name} = ${value}`));
const devStep = vi.fn((quest: string, step: string) =>
  ok(`${quest} moved to ${step}`),
);
const devSwitches = vi.fn((s: DevSwitches) => ok(s));
const setAlwaysOnTop = vi.fn((_on: boolean) => Promise.resolve());

vi.mock("@/bindings", () => ({
  commands: {
    devState: () =>
      game.state ? ok(game.state) : err("the game isn't running"),
    gameOptions: () =>
      ok({
        places: [
          "FakeHill_Child/FakeHill_FrontOfMarket",
          "FakeHill_Adult/FakeHill_FrontOfMarket",
          "FakeKakariko_Child/FakeKakariko_FrontOfGate",
        ],
        spawns: [],
        items: ["Item_A"],
        names: ["FakeFlag_IsChild", "Flag_B", ...Object.keys(values)],
        quests: { A: ["NotReady", "Step01", "Complete"] },
      }),
    devTeleport: (p: string) => devTeleport(p),
    devActors: (m: string) => devActors(m),
    devWarp: (a: string, s: string) => devWarp(a, s),
    devNearby: () => ok([]),
    devHide: (h: string[]) => ok(h),
    devApply: (n: string) => devApply(n),
    devValue: (n: string) => devValue(n),
    devSet: (n: string, v: string) => devSet(n, v),
    devStep: (q: string, s: string) => devStep(q, s),
    devSwitches: (s: DevSwitches) => devSwitches(s),
    devGoals: () => ok(goals),
    config: () => ok({ saved_tests: { [setup.name]: saved } }),
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setAlwaysOnTop }),
}));

beforeEach(() => {
  game.state = {
    map: "FakeHill_Child",
    child: true,
    loading: false,
    event: false,
    switches: { health: true, magic: false, resources: false },
  };
});

afterEach(() => {
  for (const name of Object.keys(goals)) delete goals[name];
  setup.name = "Well";
  delete saved.goals;
  cleanup();
  vi.clearAllMocks();
});

test("shows the map and age, and lists places for that age by area", async () => {
  render(<DevWindow />);
  expect(await screen.findByText("FakeHill_Child")).toBeTruthy();
  expect(screen.getByText("as child")).toBeTruthy();
  const field = await screen.findByRole("list", { name: "FakeHill" });
  expect(within(field).getAllByRole("button")).toHaveLength(1);
  expect(screen.getByRole("list", { name: "FakeKakariko" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Switch to adult" })).toBeTruthy();
});

test("a teleport says what happened, and a refusal shows inline", async () => {
  render(<DevWindow />);
  const field = await screen.findByRole("list", { name: "FakeHill" });
  const spot = within(field).getByRole("button");
  await vi.waitFor(() => expect(spot.hasAttribute("disabled")).toBe(false));
  fireEvent.click(spot);
  expect(devTeleport).toHaveBeenCalledWith(
    "FakeHill_Child/FakeHill_FrontOfMarket",
  );
  expect((await screen.findByRole("status")).textContent).toBe("sent");

  devTeleport.mockImplementationOnce(() => err("an event is playing"));
  fireEvent.click(spot);
  expect((await screen.findByRole("alert")).textContent).toBe(
    "an event is playing",
  );
});

test("the switches show what the game holds, and a toggle sets one", async () => {
  render(<DevWindow />);
  const health = await screen.findByRole("switch", { name: "Infinite health" });
  await vi.waitFor(() =>
    expect(health.getAttribute("aria-checked")).toBe("true"),
  );
  const magic = screen.getByRole("switch", { name: "Infinite magic" });
  expect(magic.getAttribute("aria-checked")).toBe("false");
  expect(
    screen
      .getByRole("switch", { name: "Infinite resources" })
      .getAttribute("aria-checked"),
  ).toBe("false");

  fireEvent.click(magic);
  expect(devSwitches).toHaveBeenCalledWith({
    health: true,
    magic: true,
    resources: false,
  });
  expect((await screen.findByRole("status")).textContent).toBe(
    "infinite magic on",
  );
  expect(magic.getAttribute("aria-checked")).toBe("true");
});

test("the search narrows the places", async () => {
  render(<DevWindow />);
  await screen.findByRole("list", { name: "FakeKakariko" });
  fireEvent.change(screen.getByRole("textbox", { name: "Search places" }), {
    target: { value: "gate" },
  });
  expect(screen.queryByRole("list", { name: "FakeHill" })).toBeNull();
  expect(screen.getByRole("list", { name: "FakeKakariko" })).toBeTruthy();
});

test("the actors tab lists the map's actors, and search narrows them", async () => {
  render(<DevWindow />);
  fireEvent.click(await screen.findByRole("button", { name: "Actors" }));
  const list = await screen.findByRole("list", {
    name: "Actors in FakeHill_Child",
  });
  expect(devActors).toHaveBeenCalledWith("FakeHill_Child");
  expect(await within(list).findByText("Field_000")).toBeTruthy();
  expect(within(list).getByText("#2")).toBeTruthy();
  expect(within(list).getAllByRole("button")).toHaveLength(3);
  const search = screen.getByRole("textbox", { name: "Search actors" });
  fireEvent.change(search, { target: { value: "npc" } });
  expect(within(list).getAllByRole("button")).toHaveLength(1);
  fireEvent.change(search, { target: { value: "field_0" } });
  expect(within(list).getAllByRole("button")).toHaveLength(1);
});

test("a click on an actor warps to the picked side", async () => {
  render(<DevWindow />);
  fireEvent.click(await screen.findByRole("button", { name: "Actors" }));
  const zelda = await screen.findByRole("button", {
    name: "Npc_A",
  });
  await vi.waitFor(() => expect(zelda.hasAttribute("disabled")).toBe(false));
  fireEvent.click(screen.getByRole("button", { name: "Back" }));
  fireEvent.click(zelda);
  expect(devWarp).toHaveBeenCalledWith("1", "back");
  expect((await screen.findByRole("status")).textContent).toBe(
    "warped beside 1",
  );
});

test("without a map the actors tab waits for one", async () => {
  game.state = undefined;
  render(<DevWindow />);
  fireEvent.click(await screen.findByRole("button", { name: "Actors" }));
  const teleport = screen.getByRole("region", { name: "Teleport" });
  expect(within(teleport).getByText("Waiting for the map")).toBeTruthy();
  expect(devActors).not.toHaveBeenCalled();
});

test("a name field suggests matches under itself, picked by click or Enter", async () => {
  render(<DevWindow />);
  const item = await screen.findByRole("textbox", { name: "Item" });
  await screen.findByRole("list", { name: "FakeKakariko" });
  fireEvent.change(item, { target: { value: "item_a" } });
  const list = screen.getByRole("list", { name: "Item suggestions" });
  fireEvent.click(within(list).getByRole("button", { name: "Item_A" }));
  expect((item as HTMLInputElement).value).toBe("Item_A");
  expect(screen.queryByRole("list", { name: "Item suggestions" })).toBeNull();

  const name = screen.getByRole("textbox", { name: "GameData name" });
  fireEvent.change(name, { target: { value: "child" } });
  fireEvent.keyDown(name, { key: "Enter" });
  expect((name as HTMLInputElement).value).toBe("FakeFlag_IsChild");
});

test("with no game, it says why and nothing can be sent", async () => {
  game.state = undefined;
  render(<DevWindow />);
  expect(await screen.findByText("the game isn't running")).toBeTruthy();
  const field = await screen.findByRole("list", { name: "FakeHill" });
  for (const b of within(field).getAllByRole("button")) {
    expect(b.hasAttribute("disabled")).toBe(true);
  }
  expect(
    screen.getByRole("button", { name: "Apply now" }).hasAttribute("disabled"),
  ).toBe(true);
});

test("the pin takes the window off the top and back", async () => {
  render(<DevWindow />);
  fireEvent.click(screen.getByRole("button", { name: "Unpin from the top" }));
  await vi.waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalledWith(false));
  fireEvent.click(await screen.findByRole("button", { name: "Keep on top" }));
  await vi.waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalledWith(true));
});

test("a saved test start applies live and lists what it did", async () => {
  render(<DevWindow />);
  const apply = await screen.findByRole("button", { name: "Apply now" });
  await vi.waitFor(() => expect(apply.hasAttribute("disabled")).toBe(false));
  fireEvent.click(apply);
  expect(devApply).toHaveBeenCalledWith("Well");
  expect((await screen.findByRole("status")).textContent).toBe(
    "gave Item_A\nsent to X/Y",
  );
});

test("a start ticks once its goals hold, marking ones held before Apply", async () => {
  saved.goals = ["Seen=true", "Lens=true", "Nope=true"];
  goals.Well = [
    goal("Seen", true),
    goal("Lens", false),
    goal("Nope", false, "no entry"),
  ];
  render(<DevWindow />);
  expect(await screen.findByLabelText("Goals open")).toBeTruthy();
  expect(screen.getByText("(no entry)")).toBeTruthy();
  expect(screen.getByText("a lens")).toBeTruthy();

  devApply.mockReturnValueOnce(ok({ done: ["sent to X/Y"], held: ["Seen"] }));
  const apply = await screen.findByRole("button", { name: "Apply now" });
  await vi.waitFor(() => expect(apply.hasAttribute("disabled")).toBe(false));
  fireEvent.click(apply);
  expect((await screen.findByRole("status")).textContent).toBe(
    "sent to X/Y\nSeen held before Apply",
  );
  saved.goals = ["Seen=true", "Lens=true"];
  goals.Well = [goal("Seen", true), goal("Lens", true)];
  const met = await screen.findByLabelText("Goals met", {}, { timeout: 2000 });
  expect(met.className).toContain("text-gold-ink");
  expect(screen.getAllByText("held before Apply")).toHaveLength(1);
});

test("a start shows its goals from the config before the game answers", async () => {
  saved.goals = ["IsGet.Item_B=true"];
  game.state = undefined;
  render(<DevWindow />);
  expect(await screen.findByLabelText("Goals open")).toBeTruthy();
  expect(screen.getByText(/IsGet.Item_B=true/)).toBeTruthy();
});

test("a start whose name holds a dot still shows its goals and holds", async () => {
  setup.name = "Well v1.2";
  saved.goals = ["Seen=true", "Lens=true"];
  goals[setup.name] = [goal("Seen", true), goal("Lens", false)];
  render(<DevWindow />);
  expect(await screen.findByLabelText("Goals open")).toBeTruthy();
  expect(screen.getByText("Well v1.2")).toBeTruthy();
  expect(screen.getByText("a lens")).toBeTruthy();

  devApply.mockReturnValueOnce(ok({ done: [], held: ["Seen"] }));
  const apply = await screen.findByRole("button", { name: "Apply now" });
  await vi.waitFor(() => expect(apply.hasAttribute("disabled")).toBe(false));
  fireEvent.click(apply);
  expect(devApply).toHaveBeenCalledWith("Well v1.2");
  expect(await screen.findByText("held before Apply")).toBeTruthy();
});

/** Types a name into Set and waits for its field, which `ready` says is
 * the one for that name. */
async function pick(name: string, ready: (field: HTMLElement) => boolean) {
  const field = await screen.findByRole("textbox", { name: "GameData name" });
  fireEvent.change(field, { target: { value: name } });
  await vi.waitFor(() =>
    expect(ready(screen.getByLabelText("Value"))).toBe(true),
  );
  return screen.getByLabelText("Value") as HTMLInputElement & HTMLSelectElement;
}

const isSelect = (f: HTMLElement) => f.tagName === "SELECT";

test("picking a name shows its live value in a field for its type", async () => {
  render(<DevWindow />);
  const bool = await pick(
    "FakeHill_ArriveEntrance",
    (f) => f.getAttribute("role") === "switch",
  );
  expect(bool.getAttribute("aria-checked")).toBe("true");

  const step = await pick(
    "Step_FakeCastle",
    (f) =>
      f.tagName === "SELECT" && (f as HTMLSelectElement).options.length === 4,
  );
  expect(step.value).toBe("Step01");
  expect([...step.options].map((o) => o.value)).toEqual([
    "NotReady",
    "Ready",
    "Step01",
    "Complete",
  ]);

  const rupees = await pick(
    "PlayerStatus.CurrentRupee",
    (f) => f.tagName === "INPUT",
  );
  expect(rupees.type).toBe("number");
  expect(rupees.value).toBe("42");

  const text = await pick(
    "FakeFlag_CurrentMap",
    (f) => (f as HTMLInputElement).type === "text",
  );
  expect(text.value).toBe("FakeHill_Child");

  const array = await pick("Fake_Array", (f) => f.tagName === "TEXTAREA");
  expect(array.value).toBe("[1, 2]");
  expect(array.hasAttribute("readonly")).toBe(true);
  expect(screen.queryByRole("button", { name: "Set" })).toBeNull();
});

test("without the game the field shows the default and says so", async () => {
  devValue.mockImplementationOnce(() =>
    ok({
      ...values.Step_FakeCastle,
      game: false,
      live: null,
    } as DevValue),
  );
  render(<DevWindow />);
  const step = await pick("Step_FakeCastle", isSelect);
  expect(step.value).toBe("NotReady");
  expect(screen.getByText(/isn't running, showing the default/)).toBeTruthy();
});

test("Set sends the picked value and reads the field again", async () => {
  render(<DevWindow />);
  const step = await pick("Step_FakeCastle", isSelect);
  expect(step.value).toBe("Step01");
  const set = screen.getByRole("button", { name: "Set" });
  await vi.waitFor(() => expect(set.hasAttribute("disabled")).toBe(false));
  fireEvent.change(step, { target: { value: "Complete" } });
  fireEvent.click(set);
  expect(devSet).toHaveBeenCalledWith("Step_FakeCastle", "Complete");
  expect((await screen.findByRole("status")).textContent).toBe(
    "Step_FakeCastle = Complete",
  );
  await vi.waitFor(() => expect(devValue).toHaveBeenCalledTimes(2));
});

test("Set's result goes once the name or value is edited", async () => {
  render(<DevWindow />);
  const rupees = await pick(
    "PlayerStatus.CurrentRupee",
    (f) => f.tagName === "INPUT",
  );
  const set = screen.getByRole("button", { name: "Set" });
  await vi.waitFor(() => expect(set.hasAttribute("disabled")).toBe(false));
  fireEvent.change(rupees, { target: { value: "200" } });
  fireEvent.click(set);
  await screen.findByRole("status");
  fireEvent.change(screen.getByLabelText("Value"), {
    target: { value: "150" },
  });
  expect(screen.queryByRole("status")).toBeNull();

  fireEvent.click(set);
  await screen.findByRole("status");
  // Picking the name again starts from typing part of it.
  fireEvent.change(screen.getByRole("textbox", { name: "GameData name" }), {
    target: { value: "PlayerStatus" },
  });
  await pick("PlayerStatus.CurrentRupee", (f) => f.tagName === "INPUT");
  expect(screen.queryByRole("status")).toBeNull();
});

test("a refused value shows the reason and an unknown name shows why", async () => {
  devSet.mockImplementationOnce(() => err("Step1 isn't a value of the name"));
  render(<DevWindow />);
  await pick("PlayerStatus.CurrentRupee", (f) => f.tagName === "INPUT");
  const set = screen.getByRole("button", { name: "Set" });
  await vi.waitFor(() => expect(set.hasAttribute("disabled")).toBe(false));
  fireEvent.click(set);
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Step1 isn't a value of the name",
  );
  fireEvent.change(screen.getByRole("textbox", { name: "GameData name" }), {
    target: { value: "0xdeadbeef" },
  });
  expect(await screen.findByText("no entry named 0xdeadbeef")).toBeTruthy();
});

test("the picked tab, searches and Give's draft are kept in the view", async () => {
  render(<DevWindow />);
  await screen.findByRole("list", { name: "FakeKakariko" });
  fireEvent.change(screen.getByRole("textbox", { name: "Search places" }), {
    target: { value: "gate" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Item" }), {
    target: { value: "Item_A" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Actors" }));
  cleanup();

  render(<DevWindow />);
  expect(await screen.findByRole("list", { name: /^Actors in/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Places" }));
  expect(
    (screen.getByRole("textbox", { name: "Search places" }) as HTMLInputElement)
      .value,
  ).toBe("gate");
  expect(
    (screen.getByRole("textbox", { name: "Item" }) as HTMLInputElement).value,
  ).toBe("Item_A");
  expect(useUi.getState().dev.give.item).toBe("Item_A");
});

test("a poll that finds the game unchanged keeps what it held", async () => {
  vi.useFakeTimers();
  try {
    render(<DevWindow />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    const held = useDev.getState().game;
    expect(held.state?.map).toBe("FakeHill_Child");
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(useDev.getState().game).toBe(held);
  } finally {
    vi.useRealTimers();
  }
});

test("a formula shows how it's worked out, its names picking each to set", async () => {
  render(<DevWindow />);
  const field = await screen.findByRole("textbox", { name: "GameData name" });
  fireEvent.change(field, { target: { value: "Flag_A_Exp" } });
  expect(await screen.findByText(/set those instead/)).toBeTruthy();
  expect(screen.queryByLabelText("Value")).toBeNull();
  expect(screen.queryByRole("button", { name: "Set" })).toBeNull();
  expect(screen.queryByText(/doesn't hold this value/)).toBeNull();
  // Not a name the game has: plain text.
  expect(screen.queryByRole("button", { name: "Complete" })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Step_A" }));
  expect(useUi.getPath("dev.set.name")).toBe("Step_A");
  await vi.waitFor(() =>
    expect((screen.getByLabelText("Value") as HTMLSelectElement).value).toBe(
      "Step01",
    ),
  );
});

test("a saved step says it leaves the quest, and fills Quest step to move it", async () => {
  render(<DevWindow />);
  await pick("Step_A", isSelect);
  expect(await screen.findByText(/leaves the quest where it was/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Move the quest" }));
  expect(useUi.getPath("dev.quest")).toEqual({
    challenge: "A",
    step: "Step01",
  });
});

test("Quest step moves the picked quest to the picked step", async () => {
  render(<DevWindow />);
  const quest = await screen.findByRole("textbox", { name: "Quest" });
  fireEvent.change(quest, { target: { value: "A" } });
  const step = (await screen.findByLabelText("Step")) as HTMLSelectElement;
  // Pinned, or a long step squeezes the quest field shut.
  expect(step.className).toContain("shrink-0");
  expect(step.value).toBe("NotReady");
  fireEvent.change(step, { target: { value: "Complete" } });
  const move = screen.getByRole("button", { name: "Move" });
  await vi.waitFor(() => expect(move.hasAttribute("disabled")).toBe(false));
  fireEvent.click(move);
  expect(devStep).toHaveBeenCalledWith("A", "Complete");
  expect(await screen.findByText("A moved to Complete")).toBeTruthy();
});
