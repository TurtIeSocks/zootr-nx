import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { PlandoOptions } from "@/bindings";
import { type FullConfig, full } from "@/lib/config";
import { PlandoPane } from "@/settings/plando-pane";
import { useConfig } from "@/stores/config";

const setConfig = vi.fn((_c: FullConfig) =>
  Promise.resolve({ status: "ok", data: null }),
);

const OPTIONS: PlandoOptions = {
  checks: [
    {
      hash: "11",
      area: "Kokiri Forest",
      place: "Mido's House",
      vanilla: "Blue Rupee",
      needs: null,
    },
    {
      hash: "22",
      area: "Lost Woods",
      place: "Target",
      vanilla: "Slingshot Upgrade",
      needs: null,
    },
    {
      hash: "33",
      area: "Goron City",
      place: "Bigger Bomb Bag (Rolling Goron)",
      vanilla: "Bigger Bomb Bag",
      needs: "npc-gifts",
    },
  ],
  items: [
    { actor: "Item_A", name: "Hookshot" },
    { actor: "Item_B", name: "Bombs" },
  ],
  error: null,
};
const plandoOptions = vi.fn(async () => ({
  status: "ok",
  data: OPTIONS,
}));

vi.mock("@/bindings", () => ({
  commands: {
    plandoOptions: () => plandoOptions(),
    setConfig: (c: FullConfig) => setConfig(c),
  },
}));

// Radix Select reaches for pointer capture and scrolling, which jsdom lacks.
Element.prototype.hasPointerCapture = () => false;
Element.prototype.releasePointerCapture = () => {};
Element.prototype.scrollIntoView = () => {};

afterEach(() => {
  cleanup();
  setConfig.mockClear();
  plandoOptions.mockClear();
});

function show(config: FullConfig) {
  useConfig.setPath("schema", [
    {
      id: "npc-gifts",
      group: "Shuffles",
      label: "NPC gifts",
      description: "",
      kind: { type: "toggle", default: false },
      parent: null,
      subgroup: null,
    },
  ]);
  useConfig.setPath("config", config);
  render(<PlandoPane />);
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;
const type = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
/** Picks `name` from the saved plandos dropdown. */
const pick = (name: string) => {
  fireEvent.pointerDown(screen.getByRole("combobox"), {
    button: 0,
    ctrlKey: false,
    pointerType: "mouse",
  });
  fireEvent.click(screen.getByRole("option", { name }));
};
/** What the config was last saved as. */
const saved = () => setConfig.mock.lastCall?.[0];

test("pins a check into the draft, names it in the table, and removes it", async () => {
  show(full({}));
  await waitFor(() => expect(plandoOptions).toHaveBeenCalled());
  type("Check", "Lost Woods: Target");
  type("Item", "Hookshot");
  await waitFor(() => expect(button("Pin").disabled).toBe(false));
  fireEvent.click(button("Pin"));
  await waitFor(() => expect(saved()?.place).toEqual({ 22: "Item_A" }));
  expect(await screen.findByText("Target")).toBeTruthy();
  expect(screen.getByText("Slingshot Upgrade")).toBeTruthy();
  expect(screen.getByText(/Not saved/)).toBeTruthy();
  fireEvent.click(button("Remove Target"));
  await waitFor(() => expect(saved()?.place).toEqual({}));
});

test("an unknown check or item can't be pinned", async () => {
  show(full({}));
  await waitFor(() => expect(plandoOptions).toHaveBeenCalled());
  type("Check", "Nowhere");
  type("Item", "Hookshot");
  expect(screen.getByText(/Not a check the game has/)).toBeTruthy();
  expect(button("Pin").disabled).toBe(true);
});

test("a pinned check these settings don't deal says so", async () => {
  show(full({ place: { 99: "Item_B" } }));
  expect(await screen.findByText("Not dealt with these settings")).toBeTruthy();
  expect(screen.getByText("Bombs")).toBeTruthy();
});

test("pins sort by area then check, unknown checks last", async () => {
  show(full({ place: { 99: "Item_B", 22: "Item_B", 11: "Item_B" } }));
  await screen.findByText("Target");
  const rows = screen.getAllByRole("row").slice(1);
  expect(rows.map((r) => r.children[1]?.textContent)).toEqual([
    "Mido's House",
    "Target",
    "99Not dealt with these settings",
  ]);
});

test("the deal's error shows over the table", async () => {
  plandoOptions.mockResolvedValueOnce({
    status: "ok",
    data: { ...OPTIONS, error: "Saria can't give Item_C" },
  });
  show(full({}));
  expect(await screen.findByText(/Saria can't give/)).toBeTruthy();
});

test("Save keeps the draft under its name, then has nothing to save", async () => {
  show(full({ place: { 11: "Item_B" } }));
  expect(button("Save").disabled).toBe(true);
  type("Plando name", "Bombs");
  fireEvent.click(button("Save"));
  await waitFor(() =>
    expect(saved()?.plandos).toEqual({ Bombs: { 11: "Item_B" } }),
  );
  expect(await screen.findByText("Saved as Bombs")).toBeTruthy();
  expect(button("Save").disabled).toBe(true);
  expect(screen.getByRole("combobox").textContent).toContain("Bombs");
});

test("a saved plando loads straight over saved pins and fills the name", async () => {
  show(
    full({
      place: { 11: "Item_B" },
      plandos: { A: { 11: "Item_B" }, B: { 22: "Item_A" } },
    }),
  );
  expect((screen.getByLabelText("Plando name") as HTMLInputElement).value).toBe(
    "A",
  );
  pick("B");
  await waitFor(() => expect(saved()?.place).toEqual({ 22: "Item_A" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect((screen.getByLabelText("Plando name") as HTMLInputElement).value).toBe(
    "B",
  );
});

test("loading over unsaved pins asks first", async () => {
  show(
    full({
      place: { 11: "Item_B" },
      plandos: { B: { 22: "Item_A" } },
    }),
  );
  pick("B");
  expect(await screen.findByText("Replace the unsaved pins?")).toBeTruthy();
  fireEvent.click(button("Cancel"));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(setConfig).not.toHaveBeenCalled();
  pick("B");
  fireEvent.click(await screen.findByRole("button", { name: "Replace" }));
  await waitFor(() => expect(saved()?.place).toEqual({ 22: "Item_A" }));
});

test("Delete drops the named saved plando and Clear empties the draft", async () => {
  show(
    full({
      place: { 11: "Item_B" },
      plandos: { A: { 11: "Item_B" } },
    }),
  );
  fireEvent.click(button("Delete"));
  await waitFor(() => expect(saved()?.plandos).toEqual({}));
  expect(saved()?.place).toEqual({ 11: "Item_B" });
  expect(await screen.findByText(/Not saved/)).toBeTruthy();
  fireEvent.click(button("Clear"));
  await waitFor(() => expect(saved()?.place).toEqual({}));
  expect(await screen.findByText(/No pins/)).toBeTruthy();
});

test("a check the settings leave out says which setting deals it", async () => {
  show(full({ place: { 33: "Item_B" } }));
  expect(await screen.findByText("Needs NPC gifts")).toBeTruthy();
  fireEvent.focus(screen.getByLabelText("Check"));
  type("Check", "goron");
  expect(screen.getByText("needs NPC gifts")).toBeTruthy();
  type("Check", "Goron City: Bigger Bomb Bag (Rolling Goron)");
  type("Item", "Hookshot");
  expect(button("Pin").disabled).toBe(true);
  expect(screen.getByText(/Your settings don't deal this check/)).toBeTruthy();
  fireEvent.click(button("Turn on NPC gifts"));
  await waitFor(() => expect(saved()?.settings).toEqual({ "npc-gifts": true }));
});
