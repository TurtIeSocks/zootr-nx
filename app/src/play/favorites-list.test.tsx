import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { BuiltIn, Config, Favorite } from "@/bindings";
import { type FullConfig, full } from "@/lib/config";
import { FavoritesList } from "@/play/favorites-list";
import { useConfig } from "@/stores/config";
import { toggleDef } from "@/test/fixtures";

const setConfig = vi.fn(async (c: FullConfig) =>
  c.favorites.some((f) => f.name === "Taken")
    ? { status: "error", error: "a favorite is already called Taken" }
    : { status: "ok", data: [] },
);

vi.mock("@/bindings", () => ({
  commands: {
    setConfig: (c: FullConfig) => setConfig(c),
    status: async () => ({ status: "ok", data: { game: true, targets: [] } }),
    history: async () => ({
      status: "ok",
      data: { installed: null, entries: [] },
    }),
  },
}));

afterEach(() => {
  cleanup();
  setConfig.mockClear();
});

const BUILT_IN: BuiltIn[] = [
  {
    id: "vanilla-plus",
    name: "Vanilla+",
    note: "Nothing shuffled",
    settings: { b: true },
  },
  {
    id: "randomizer",
    name: "Randomizer",
    note: "The usual setup",
    settings: { a: true, b: true },
  },
];

const mine = (
  name: string,
  settings = {},
  draft: Partial<Favorite> = {},
): Favorite => ({
  name,
  note: `${name}'s note`,
  time: 1_700_000_000,
  settings,
  ...draft,
});

/** A draft to keep or to have: pins, a palette and one music pool. */
const DRAFT = {
  place: { 22: "Item_A" },
  colors: { "kokiri-tunic": "random" as const },
  audio: { pools: { area: "shuffle" as const } },
};

function show(
  favorites: Favorite[],
  settings = {},
  draft: Partial<Config> = {},
) {
  useConfig.setPath("schema", [toggleDef("a"), toggleDef("b")]);
  useConfig.setPath("builtIns", BUILT_IN);
  useConfig.setPath("config", full({ settings, favorites, ...draft }));
  render(<FavoritesList />);
}

const list = (name: string) => within(screen.getByRole("list", { name }));
const rows = (name: string) =>
  list(name)
    .getAllByRole("listitem")
    .map((li) => li.textContent ?? "");
const saved = () => setConfig.mock.lastCall?.[0];
const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

test("hers come first, newest first, and the built-ins stay pinned below", () => {
  show([mine("New"), mine("Old")], { a: true });
  expect(rows("Your favorites")[0]).toContain("New");
  expect(rows("Your favorites")[1]).toContain("Old's note");
  expect(rows("Built in")).toEqual([
    expect.stringMatching(/^Vanilla\+Nothing shuffled· differs in 2 changes/),
    expect.stringMatching(/^RandomizerThe usual setup· differs in 1 change/),
  ]);
  const builtIn = list("Built in");
  expect(builtIn.getAllByRole("button", { name: "Use settings" })).toHaveLength(
    2,
  );
  for (const name of ["Update favorite", "Label favorite", "Delete favorite"]) {
    expect(builtIn.queryByRole("button", { name })).toBeNull();
  }
});

test("with none of her own it says how to keep one", () => {
  show([]);
  expect(screen.getByText(/No favorites yet/)).toBeTruthy();
  expect(rows("Built in")).toHaveLength(2);
});

test("use settings replaces them, and waits while they already match", async () => {
  show([mine("Mine", { a: true })], { b: true });
  const use = list("Your favorites").getByRole("button", {
    name: "Use settings",
  });
  fireEvent.click(use);
  await waitFor(() => expect(saved()?.settings).toEqual({ a: true }));
  cleanup();
  show([mine("Mine", { a: true })], { a: true });
  expect(
    list("Your favorites")
      .getByRole("button", { name: "Use settings" })
      .getAttribute("aria-disabled"),
  ).toBe("true");
});

test("use settings brings back the favorite's pins, colors and music", async () => {
  show([mine("Mine", { a: true }, DRAFT)], {
    b: true,
    place: { 99: "Item_B" },
    colors: { "kokiri-tunic": "vanilla" },
    audio: { pools: { area: "vanilla" } },
  });
  fireEvent.click(
    list("Your favorites").getByRole("button", { name: "Use settings" }),
  );
  await waitFor(() => expect(saved()?.settings).toEqual({ a: true }));
  expect(saved()?.place).toEqual({ 22: "Item_A" });
  expect(saved()?.colors).toEqual({ "kokiri-tunic": "random" });
  expect(saved()?.audio).toEqual({ pools: { area: "shuffle" } });
});

test("a favorite that kept no draft says nothing about yours", async () => {
  show([mine("Old", { a: true })], { b: true }, DRAFT);
  fireEvent.click(
    list("Your favorites").getByRole("button", { name: "Use settings" }),
  );
  await waitFor(() => expect(saved()?.settings).toEqual({ a: true }));
  expect(saved()?.place).toEqual({ 22: "Item_A" });
  expect(saved()?.colors).toEqual({ "kokiri-tunic": "random" });
  expect(saved()?.audio).toEqual({ pools: { area: "shuffle" } });
});

test("a built-in brings its settings alone", async () => {
  show([], {}, DRAFT);
  const [vanilla] = list("Built in").getAllByRole("button", {
    name: "Use settings",
  });
  fireEvent.click(vanilla as HTMLElement);
  await waitFor(() => expect(saved()?.settings).toEqual({ b: true }));
  expect(saved()?.place).toEqual({ 22: "Item_A" });
  expect(saved()?.colors).toEqual({ "kokiri-tunic": "random" });
  expect(saved()?.audio).toEqual({ pools: { area: "shuffle" } });
});

test("a favorite whose only difference is a pin still offers it", () => {
  show([mine("Mine", {}, { place: { 22: "Item_A" } })]);
  expect(rows("Your favorites")[0]).toContain("differs in 1 change");
  expect(
    list("Your favorites")
      .getByRole("button", { name: "Use settings" })
      .getAttribute("aria-disabled"),
  ).toBeNull();
});

test("save current keeps the draft under a name, at the top", async () => {
  show([mine("Old")], { a: true }, DRAFT);
  fireEvent.click(button("Save current"));
  fireEvent.change(await screen.findByLabelText("Name"), {
    target: { value: " Fresh " },
  });
  fireEvent.click(button("Save"));
  await waitFor(() =>
    expect(saved()?.favorites.map((f) => f.name)).toEqual(["Fresh", "Old"]),
  );
  expect(saved()?.favorites[0]).toMatchObject({
    settings: { a: true },
    place: { 22: "Item_A" },
    colors: { "kokiri-tunic": "random" },
    audio: { pools: { area: "shuffle" } },
  });
});

test("a taken name is refused in the dialog", async () => {
  show([]);
  fireEvent.click(button("Save current"));
  fireEvent.change(await screen.findByLabelText("Name"), {
    target: { value: "Taken" },
  });
  fireEvent.click(button("Save"));
  expect(await screen.findByText(/already called Taken/)).toBeTruthy();
  expect(screen.getByRole("dialog")).toBeTruthy();
});

test("labelling renames in place and keeps the note", async () => {
  show([mine("A"), mine("B")]);
  const [, second] = screen.getAllByRole("button", { name: "Label favorite" });
  fireEvent.click(second as HTMLElement);
  const name = (await screen.findByLabelText("Name")) as HTMLInputElement;
  expect(name.value).toBe("B");
  fireEvent.change(name, { target: { value: "C" } });
  fireEvent.click(button("Save"));
  await waitFor(() =>
    expect(saved()?.favorites.map((f) => [f.name, f.note])).toEqual([
      ["A", "A's note"],
      ["C", "B's note"],
    ]),
  );
});

test("update and delete each ask for a second click", async () => {
  show([mine("A"), mine("B", { a: true })], { b: true });
  const [, update] = screen.getAllByRole("button", { name: "Update favorite" });
  fireEvent.click(update as HTMLElement);
  expect(setConfig).not.toHaveBeenCalled();
  fireEvent.click(update as HTMLElement);
  await waitFor(() =>
    expect(saved()?.favorites.map((f) => [f.name, f.settings])).toEqual([
      ["B", { b: true }],
      ["A", {}],
    ]),
  );
  setConfig.mockClear();
  const [del] = screen.getAllByRole("button", { name: "Delete favorite" });
  fireEvent.click(del as HTMLElement);
  expect(setConfig).not.toHaveBeenCalled();
  fireEvent.click(del as HTMLElement);
  await waitFor(() => expect(saved()?.favorites).toHaveLength(1));
});
