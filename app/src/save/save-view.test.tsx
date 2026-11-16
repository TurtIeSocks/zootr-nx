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
  max_life: 12,
  child: true,
  stones: 1,
  medallions: 0,
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
      ],
      caps: [null, 20],
      staged: [edits.some((e) => e.hash === HOOK), false],
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
const saveWrite = vi.fn(
  (_slot: number, { edits }: Pending, _stamp: number): Promise<unknown> =>
    ok({ log: edits.map((e) => `#${e.hash}: ${e.value}`), backup: "b.bak" }),
);

vi.mock("@/bindings", () => ({
  TRACKER_COLUMNS: 3,
  commands: {
    saveSlots: () => saveSlots(),
    saveItems: (s: number, p: Pending) => saveItems(s, p),
    saveClick: (s: number, p: Pending, at: number, i: Intent) =>
      saveClick(s, p, at, i),
    saveFlags: (s: number, p: Pending, q: FlagQuery) => saveFlags(s, p, q),
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
  expect(slot2.textContent).toContain("3 hearts");
  expect(slot2.textContent).toContain("Continue");
});

test("a click goes up a tier, a right click down, and the tile shows a dot", async () => {
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
  fireEvent.contextMenu(tile("Hookshot"));
  await waitFor(() =>
    expect(useUi.getPath("save.edits")).toEqual([
      { hash: HOOK, index: 0, value: "0" },
    ]),
  );
  expect(screen.getByText("1 change to slot 2")).toBeTruthy();
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

test("Discard drops what's pending", async () => {
  await open();
  fireEvent.click(tile("Hookshot"));
  await waitFor(() => expect(useUi.getPath("save.edits")).toHaveLength(1));
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
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
  fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
  expect(useUi.getPath("save.slot")).toBe(2);
  fireEvent.click(screen.getByRole("radio", { name: /Slot 0/ }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Discard and switch" }),
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
