import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type {
  Cell,
  Edit,
  FlagQuery,
  Intent,
  ItemsView,
  Pending,
  SlotRow,
} from "@/bindings";
import { SaveView } from "@/save/save-view";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

/** The made-up save's hashes. */
const HOOK = 1;
const COUNT = 2;
const FLAG = 3;

const row = (slot: number, stamp: number, more: Partial<SlotRow> = {}) => ({
  slot,
  stamp,
  continues: false,
  child: true,
  stones: [],
  medallions: [],
  stats: [
    { label: "Hearts", icon: "h", glyph: "heart" as const, value: 12, per: 4 },
    { label: "Rupees", icon: "r", glyph: "rupee" as const, value: 99, per: 1 },
  ],
  error: null,
  ...more,
});

/** Each slot's stamp, which a test moves to stand for the game writing. */
const stamps = new Map([
  [2, 2_000],
  [0, 1_000],
]);
const saveSlots = vi.fn(() =>
  ok([
    row(2, stamps.get(2) ?? 0, { continues: true }),
    row(0, stamps.get(0) ?? 0),
  ]),
);

const cell = (label: string, more: Partial<Cell>): Cell => ({
  section: "Inventory",
  label,
  icon: label,
  glyph: "unknown",
  tier: 0,
  tiers: 1,
  count: null,
  row: null,
  ...more,
});

/** The value an edit list leaves `hash` at, else `none`. */
const last = (edits: Edit[], hash: number, none: string) =>
  edits.filter((e) => e.hash === hash).at(-1)?.value ?? none;

const saveItems = vi.fn(
  (_slot: number, { edits }: Pending): Promise<unknown> => {
    const tier = Number(last(edits, HOOK, "0"));
    const view: ItemsView = {
      sections: [{ section: "Inventory", column: 1, width: 6, shown: true }],
      cells: [
        cell("Hookshot", { tier, tiers: 2 }),
        cell("Bombs", { tier: 1, count: Number(last(edits, COUNT, "5")) }),
        cell("Bottle", { tier: 1 }),
        cell("Bottle", { tier: 0 }),
      ],
      caps: [null, 20, null, null],
      staged: [edits.some((e) => e.hash === HOOK), false, false, false],
      status: [
        {
          group: "Meters",
          label: "Heart containers",
          hash: 9,
          input: "number",
          value: last(edits, 9, "12"),
          choices: [],
          per: 4,
          max: 80,
          warn: null,
        },
      ],
    };
    return ok(view);
  },
);
const saveClick = vi.fn(
  (_slot: number, { edits }: Pending, at: number, intent: Intent) => {
    if (typeof intent === "object") {
      return ok([{ hash: COUNT, index: 0, value: String(intent.count) }]);
    }
    const now = Number(last(edits, HOOK, "0"));
    const next = intent === "up" ? (now + 1) % 3 : (now + 2) % 3;
    return ok(at === 0 ? [{ hash: HOOK, index: 0, value: String(next) }] : []);
  },
);
const saveFlags = vi.fn((_slot: number, { edits }: Pending, q: FlagQuery) =>
  ok({
    kinds: ["Bool"],
    groups: [
      {
        name: "FakeHill",
        count: 1,
        changed: 0,
        open: "FakeHill" in q.open,
        rows:
          "FakeHill" in q.open
            ? [
                {
                  hash: FLAG,
                  name: "FakeHill_Door",
                  label: null,
                  kind: "Bool",
                  input: "choice",
                  choices: ["true", "false"],
                  values: [last(edits, FLAG, "false")],
                  size: 4,
                  default: "false",
                  changed: false,
                  staged: edits.some((e) => e.hash === FLAG),
                },
              ]
            : [],
      },
    ],
  }),
);
/** The made-up save's values before any edit. */
const ORIGINAL: Record<number, string> = {
  [HOOK]: "0",
  [COUNT]: "5",
  [FLAG]: "false",
  9: "12",
};
/** Keeps the edits that change the save, one change each, as the backend
 * does for values no cell shares. */
const saveStage = vi.fn((_slot: number, { edits }: Pending) => {
  const kept = edits.filter((e) => ORIGINAL[e.hash] !== e.value);
  return ok({ edits: kept, count: kept.length });
});
const saveWrite = vi.fn(
  (_slot: number, { edits }: Pending, _stamp: number): Promise<unknown> =>
    ok({ log: edits.map((e) => `#${e.hash}: ${e.value}`), backup: "b.bak" }),
);

vi.mock("@/bindings", () => ({
  commands: {
    saveSlots: () => saveSlots(),
    saveItems: (s: number, p: Pending) => saveItems(s, p),
    saveClick: (s: number, p: Pending, at: number, i: Intent) =>
      saveClick(s, p, at, i),
    saveFlags: (s: number, p: Pending, q: FlagQuery) => saveFlags(s, p, q),
    saveStage: (s: number, p: Pending) => saveStage(s, p),
    savePresets: () =>
      Promise.resolve([{ name: "fake-done", label: "Fake done", what: "x" }]),
    saveWrite: (s: number, p: Pending, stamp: number) => saveWrite(s, p, stamp),
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  stamps.set(2, 2_000);
});

const tile = (label: string) =>
  screen.getByRole("button", { name: new RegExp(`^${label}`) });

/** The page, once the newest slot's grid is in. */
async function open() {
  render(<SaveView />);
  await waitFor(() => expect(tile("Hookshot")).toBeTruthy());
}

test("picks the newest slot and marks the one Continue loads", async () => {
  await open();
  expect(useUi.getPath("save.slot")).toBe(2);
  expect(useUi.getPath("save.stamp")).toBe(2_000);
  const slot2 = screen.getByRole("radio", { name: /Slot 2/ });
  expect(slot2.getAttribute("aria-checked")).toBe("true");
  expect(slot2.textContent).toContain("3 Hearts");
  expect(slot2.textContent).toContain("99 Rupees");
  expect(slot2.textContent).toContain("Continue");
});

test("a click goes up a tier and the tile shows a dot, and a right click back unstages it", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  expect(useUi.getPath("save.edits")).toEqual([
    { hash: HOOK, index: 0, value: "1" },
  ]);
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: /Hookshot.*changed/ }),
    ).toBeTruthy(),
  );
  expect(screen.getByText("1 change to slot 2")).toBeTruthy();
  fireEvent.contextMenu(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toEqual([]));
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
});

test("Backspace on a focused tile goes down a tier, as a right click does", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  fireEvent.keyDown(tile("Hookshot"), { key: "Backspace" });
  await waitFor(() => expect(useUi.getPath("save.edits")).toEqual([]));
  fireEvent.keyDown(tile("Hookshot"), { key: "Enter" });
  expect(useUi.getPath("save.edits")).toEqual([]);
});

test("tiles that share a label get a number, so each button has its own name", async () => {
  await open();
  expect(screen.getByRole("button", { name: /^Bottle 1/ })).toBeTruthy();
  expect(
    screen.getByRole("button", { name: /^Bottle 2 \(missing\)/ }),
  ).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Bottle \(/ })).toBeNull();
  expect(screen.getByRole("button", { name: /^Hookshot/ })).toBeTruthy();
});

test("the count opens a field that stages how many", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Bombs: 5" }));
  const field = await screen.findByLabelText("How many Bombs");
  expect(field.getAttribute("max")).toBe("20");
  fireEvent.change(field, { target: { value: "12" } });
  fireEvent.submit(field);
  await waitFor(() =>
    expect(useUi.getPath("save.edits")).toEqual([
      { hash: COUNT, index: 0, value: "12" },
    ]),
  );
});

test("hearts show whole and stage in quarters", async () => {
  await open();
  const hearts = screen.getByLabelText("Heart containers") as HTMLInputElement;
  expect(hearts.value).toBe("3");
  fireEvent.change(hearts, { target: { value: "4.5" } });
  fireEvent.keyDown(hearts, { key: "Enter" });
  await waitFor(() =>
    expect(useUi.getPath("save.edits")).toEqual([
      { hash: 9, index: 0, value: "18" },
    ]),
  );
});

test("a flag row stages its value, and Save writes what's pending", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Flags" }));
  fireEvent.click(await screen.findByRole("button", { name: /FakeHill/ }));
  const door = await screen.findByRole("switch", { name: "FakeHill_Door" });
  fireEvent.click(door);
  await waitFor(() =>
    expect(useUi.getPath("save.edits")).toEqual([
      { hash: FLAG, index: 0, value: "true" },
    ]),
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(saveWrite).toHaveBeenCalled());
  expect(saveWrite.mock.calls[0]?.[0]).toBe(2);
  expect(saveWrite.mock.calls[0]?.[2]).toBe(2_000);
  await waitFor(() => expect(screen.getByText(/Backup at b.bak/)).toBeTruthy());
  expect(useUi.getPath("save.edits")).toEqual([]);
});

test("the Flags tab says types in plain words and filters by them", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Flags" }));
  expect(
    await screen.findByLabelText("Filter flags by name, hash or label"),
  ).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Type"), {
    target: { value: "On/Off" },
  });
  await waitFor(() =>
    expect(saveFlags.mock.lastCall?.[2].kinds).toEqual(["Bool"]),
  );
  fireEvent.click(await screen.findByRole("button", { name: /FakeHill/ }));
  await screen.findByRole("switch", { name: "FakeHill_Door" });
  expect(screen.getAllByText("On/Off")).toHaveLength(2);
});

test("Save waits while the game runs", async () => {
  useSeeds.setPath("running", true);
  await open();
  fireEvent.click(tile("Hookshot"));
  const save = await screen.findByRole("button", { name: "Save" });
  expect(save.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(save);
  expect(saveWrite).not.toHaveBeenCalled();
  useSeeds.setPath("running", false);
});

test("Discard drops what's pending", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  expect(useUi.getPath("save.edits")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Really discard?" }));
  expect(useUi.getPath("save.edits")).toEqual([]);
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
});

test("a save the game wrote since asks to reload instead of writing", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  stamps.set(2, 3_000);
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByText(/The game wrote slot 2 since it was read/);
  expect(saveWrite).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  expect(useUi.getPath("save.stamp")).toBe(3_000);
  expect(useUi.getPath("save.edits")).toHaveLength(1);
});

test("switching slots with edits pending asks first", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  fireEvent.click(screen.getByRole("radio", { name: /Slot 0/ }));
  expect(await screen.findByText("Discard 1 change?")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Keep Editing" }));
  expect(useUi.getPath("save.slot")).toBe(2);
  fireEvent.click(screen.getByRole("radio", { name: /Slot 0/ }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Discard and Switch" }),
  );
  expect(useUi.getPath("save.slot")).toBe(0);
  expect(useUi.getPath("save.edits")).toEqual([]);
});

test("a preset stages and unstages", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Presets" }));
  const preset = await screen.findByLabelText("Fake done");
  fireEvent.click(preset);
  expect(useUi.getPath("save.presets")).toEqual(["fake-done"]);
  fireEvent.click(preset);
  expect(useUi.getPath("save.presets")).toEqual([]);
});
