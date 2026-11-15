import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Spoiler, Spot, Summary } from "@/bindings";
import { oldSeed, seed } from "@/play/spoiler/fixture";
import { SpoilerSheet } from "@/play/spoiler/spoiler-sheet";
import { useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";

const backend = {
  spoiler: seed() as Spoiler,
  /** Answers a load by hand instead, when set. */
  load: undefined as ((id: number) => Promise<Spoiler>) | undefined,
};

vi.mock("@/bindings", () => ({
  commands: {
    spoiler: async (id: number) => ({
      status: "ok",
      data: backend.load ? await backend.load(id) : backend.spoiler,
    }),
  },
}));

afterEach(() => {
  cleanup();
  backend.spoiler = seed();
  backend.load = undefined;
});

const entry = { id: 7, seed: "482913" } as Summary;

async function open(spoiler = seed()) {
  backend.spoiler = spoiler;
  useShell.setPath("spoiler", entry);
  render(<SpoilerSheet />);
  return within(await screen.findByRole("dialog"));
}

const tab = (name: string) => screen.getByRole("button", { name });
const detail = () =>
  within(screen.getByRole("region", { name: "Check detail" }));

test("Spheres shows a column a round, and picking a card links its rule", async () => {
  const sheet = await open();
  await sheet.findByText("Round 4");
  const round2 = within(sheet.getByRole("list", { name: "Round 2" }));
  expect(round2.getByText("Beat King Dodongo")).toBeTruthy();
  expect(sheet.queryByText("Green Rupee")).toBeNull();
  fireEvent.click(round2.getByRole("button", { name: /Bomb Bag/ }));
  expect(detail().getByText(/Dodongo's Cavern · Map/)).toBeTruthy();
  const link = detail().getByRole("button", {
    name: "Fairy Slingshot (round 1)",
  });
  expect(
    detail().getByRole("button", { name: "Megaton Hammer (round 4)" }),
  ).toBeTruthy();
  fireEvent.click(link);
  expect(detail().getByText(/Kokiri Forest · Mido's House/)).toBeTruthy();
  expect(detail().getByText("always")).toBeTruthy();
});

test("a picked card makes the cards giving what it needs glow", async () => {
  const sheet = await open();
  const round = async (n: number) =>
    within(await sheet.findByRole("list", { name: `Round ${n}` }));
  const card = async (n: number, name: RegExp) =>
    (await round(n)).getByRole("button", { name });
  const glows = (el: HTMLElement) => el.getAttribute("data-glow");
  fireEvent.click(await card(2, /Bomb Bag/));
  // Slingshot and (Hammer or Bomb Bag): the hammer is one option.
  expect(glows(await card(1, /Fairy Slingshot ?\(needed\)/))).toBe("all");
  const hammer = await card(4, /Megaton Hammer ?\(one option\)/);
  expect(glows(hammer)).toBe("any");
  expect(hammer.className).toContain("data-[glow=any]:border-dashed");
  expect(glows(await card(1, /Fairy Bow/))).toBeNull();
  expect(glows(await card(2, /Bomb Bag/))).toBeNull();
  // Another pick moves the glow, and what it unlocks glows blue.
  fireEvent.click(await card(4, /Megaton Hammer/));
  expect(glows(await card(1, /Fairy Slingshot/))).toBeNull();
  expect(glows(await card(2, /Bomb Bag ?\(unlocked\)/))).toBe("unlocked");
});

test("a picked card makes a Start item it needs glow", async () => {
  const spoiler = seed();
  spoiler.spots[1] = {
    ...(spoiler.spots[1] as Spot),
    need: "Item_H",
    need_items: ["Item_H"],
  };
  const sheet = await open(spoiler);
  const round3 = within(await sheet.findByRole("list", { name: "Round 3" }));
  const ocarina = () =>
    within(sheet.getByRole("list", { name: "Start" }))
      .getByText("Fairy Ocarina")
      .closest("li") as HTMLElement;
  expect(ocarina().getAttribute("data-glow")).toBeNull();
  fireEvent.click(round3.getByRole("button", { name: /Zelda's Lullaby/ }));
  expect(ocarina().getAttribute("data-glow")).toBe("all");
  expect(within(ocarina()).getByText("(needed)")).toBeTruthy();
});

test("pressing the picked card again clears the pick and its glow", async () => {
  const sheet = await open();
  const round2 = within(await sheet.findByRole("list", { name: "Round 2" }));
  const bag = () => round2.getByRole("button", { name: /Bomb Bag/ });
  fireEvent.click(bag());
  expect(bag().getAttribute("aria-pressed")).toBe("true");
  expect(screen.getByRole("region", { name: "Check detail" })).toBeTruthy();
  fireEvent.click(bag());
  expect(bag().getAttribute("aria-pressed")).toBe("false");
  expect(screen.queryByRole("region", { name: "Check detail" })).toBeNull();
  expect(sheet.queryAllByText(/\(needed\)/)).toHaveLength(0);
});

test("Spheres starts with what Link has before round 1", async () => {
  const sheet = await open();
  const start = within(await sheet.findByRole("list", { name: "Start" }));
  expect(start.getByText("Fairy Ocarina")).toBeTruthy();
  expect(start.getByText("Open Forest")).toBeTruthy();
});

test("a seed from before the start was kept has no Start column", async () => {
  const sheet = await open({ ...seed(), start: { items: [], settings: [] } });
  await sheet.findByText("Round 4");
  expect(sheet.queryByRole("list", { name: "Start" })).toBeNull();
});

test("Spheres keeps only the Way of the Hero with the toggle on", async () => {
  const sheet = await open();
  await sheet.findByText("Round 4");
  fireEvent.click(
    sheet.getByRole("checkbox", { name: "Way of the Hero only" }),
  );
  expect(sheet.queryByText("Round 4")).toBeNull();
  expect(sheet.queryByText("Fairy Bow")).toBeNull();
  expect(sheet.queryByText("Beat King Dodongo")).toBeNull();
  expect(sheet.getByText("Fairy Slingshot")).toBeTruthy();
});

test("Way of the Hero lists its checks by area with their rounds", async () => {
  const sheet = await open();
  await sheet.findByText("Round 4");
  fireEvent.click(tab("Way of the Hero"));
  const castle = within(sheet.getByRole("list", { name: "Hyrule Castle" }));
  expect(castle.getByText("Zelda's Lullaby")).toBeTruthy();
  expect(castle.getByText("Round 3")).toBeTruthy();
  expect(sheet.queryByText("Megaton Hammer")).toBeNull();
  fireEvent.click(castle.getByRole("button", { name: /Courtyard/ }));
  expect(detail().getByText(/Hyrule Castle · Courtyard/)).toBeTruthy();
});

test("All checks has the round, the star and the rule, then the filler", async () => {
  const sheet = await open();
  await sheet.findByText("Round 4");
  fireEvent.click(tab("All checks"));
  const row = sheet.getByText("Map").closest("tr") as HTMLElement;
  expect(within(row).getByText("2")).toBeTruthy();
  expect(within(row).getByLabelText("Way of the Hero")).toBeTruthy();
  expect(
    within(row).getByText("Fairy Slingshot and Megaton Hammer or Bomb Bag"),
  ).toBeTruthy();
  expect(sheet.getByText("Spirit Temple: Upper Chest")).toBeTruthy();
  fireEvent.change(sheet.getByPlaceholderText("Search areas and items"), {
    target: { value: "lost" },
  });
  expect(sheet.queryByText("Map")).toBeNull();
  expect(sheet.getByText("Stump")).toBeTruthy();
  fireEvent.click(sheet.getByText("Stump"));
  expect(
    detail().getByRole("button", { name: "Megaton Hammer (round 4)" }),
  ).toBeTruthy();
});

test("a seed from before the logic fields shows only its checks, with a note", async () => {
  const sheet = await open(oldSeed());
  await sheet.findByText(/predates/);
  expect(screen.queryByRole("button", { name: "Spheres" })).toBeNull();
  expect(sheet.getByText("Mido's House")).toBeTruthy();
  expect(sheet.getByText("6 of 6 checks")).toBeTruthy();
});

test("the tab, the hero filter and the search are kept for the next spoiler", async () => {
  const sheet = await open();
  await sheet.findByText("Round 4");
  fireEvent.click(
    sheet.getByRole("checkbox", { name: "Way of the Hero only" }),
  );
  fireEvent.click(tab("All checks"));
  fireEvent.change(sheet.getByPlaceholderText("Search areas and items"), {
    target: { value: "bow" },
  });
  fireEvent.keyDown(sheet.getByPlaceholderText("Search areas and items"), {
    key: "Escape",
  });
  expect(useShell.getState().spoiler).toBeUndefined();
  cleanup();

  const again = await open();
  expect(tab("All checks").getAttribute("aria-pressed")).toBe("true");
  expect(
    (again.getByPlaceholderText("Search areas and items") as HTMLInputElement)
      .value,
  ).toBe("bow");
  expect(useUi.getState().spoiler).toEqual({
    tab: "checks",
    heroOnly: true,
    query: "bow",
  });
});

test("a slow load for a seed no longer shown is dropped", async () => {
  const answers = new Map<number, (s: Spoiler) => void>();
  backend.load = (id) => new Promise((resolve) => answers.set(id, resolve));
  useShell.setPath("spoiler", entry);
  render(<SpoilerSheet />);
  await vi.waitFor(() => expect(answers.has(7)).toBe(true));
  act(() => useShell.setPath("spoiler", { id: 8, seed: "1" } as Summary));
  await vi.waitFor(() => expect(answers.has(8)).toBe(true));
  await act(async () => answers.get(8)?.(seed()));
  await act(async () => answers.get(7)?.(oldSeed()));
  expect(screen.queryByText(/predates/)).toBeNull();
  expect(screen.getByRole("button", { name: "Spheres" })).toBeTruthy();
});
