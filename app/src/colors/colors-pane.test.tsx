import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { PaletteView } from "@/bindings";
import { ColorsPane } from "@/colors/colors-pane";
import { type FullConfig, full } from "@/lib/config";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

const setConfig = vi.fn((_c: FullConfig) =>
  Promise.resolve({ status: "ok", data: [] }),
);

const palette = (
  id: string,
  label: string,
  group: "link" | "gear",
  problem: string | null = null,
): PaletteView => ({
  id,
  label,
  group,
  vanilla: ["#40522b"],
  textures: 2,
  problem,
});

const CATALOG = [
  palette("kokiri-tunic", "Kokiri Tunic", "link"),
  palette("leather", "Leather", "link"),
  palette("goron-tunic", "Goron Tunic", "link", "the dump's colour differs"),
  palette("bow", "Fairy Bow", "gear"),
];

vi.mock("@/bindings", () => ({
  commands: {
    colorsCatalog: async () => ({ status: "ok", data: CATALOG }),
    colorsDraw: async (ids: string[]) => ({
      status: "ok",
      data: Object.fromEntries(ids.map((id) => [id, "#123456"])),
    }),
    setConfig: (c: FullConfig) => setConfig(c),
  },
}));

Element.prototype.hasPointerCapture = () => false;
Element.prototype.releasePointerCapture = () => {};
Element.prototype.scrollIntoView = () => {};

afterEach(() => {
  cleanup();
  setConfig.mockClear();
});

function show(config: FullConfig) {
  useSeeds.setPath("status", { game: true, targets: [] });
  useConfig.setPath("config", config);
  render(<ColorsPane />);
}

const saved = () => setConfig.mock.lastCall?.[0];
const within = (label: string) =>
  screen.getByText(label).closest("li") as HTMLElement;
const press = (row: string, name: string) =>
  fireEvent.click(
    [...within(row).querySelectorAll("button")].find(
      (b) => b.textContent === name || b.getAttribute("aria-label") === name,
    ) as HTMLElement,
  );

test("lists a group's palettes and saves a mode", async () => {
  show(full({}));
  expect(await screen.findByText("Kokiri Tunic")).toBeTruthy();
  expect(screen.queryByText("Fairy Bow")).toBeNull();
  press("Kokiri Tunic", "Random");
  await waitFor(() =>
    expect(saved()?.colors).toEqual({ "kokiri-tunic": "random" }),
  );
  expect(screen.getByText(/can't be painted here/)).toBeTruthy();
});

test("Reset All leaves a locked palette and one this dump can't paint", async () => {
  show(
    full({
      colors: { "kokiri-tunic": "random", leather: { hex: "#ff0000" } },
      color_locks: ["leather"],
    }),
  );
  await screen.findByText("Kokiri Tunic");
  fireEvent.click(screen.getByRole("button", { name: "Reset All" }));
  await waitFor(() =>
    expect(saved()?.colors).toEqual({ leather: { hex: "#ff0000" } }),
  );
});

test("Randomize All sets each unlocked palette to Random", async () => {
  show(full({ color_locks: ["bow"] }));
  await screen.findByText("Kokiri Tunic");
  fireEvent.click(screen.getByRole("button", { name: "Randomize All" }));
  await waitFor(() =>
    expect(saved()?.colors).toEqual({
      "kokiri-tunic": "random",
      leather: "random",
    }),
  );
});

test("shift-click Randomize All draws colours now", async () => {
  show(full({}));
  await screen.findByText("Kokiri Tunic");
  fireEvent.click(screen.getByRole("button", { name: "Randomize All" }), {
    shiftKey: true,
  });
  await waitFor(() => expect(saved()?.colors?.bow).toEqual({ hex: "#123456" }));
  expect(saved()?.colors?.["goron-tunic"]).toBeUndefined();
});

test("previews need game assets", async () => {
  show(full({}));
  expect(await screen.findByText(/Turn on game assets/)).toBeTruthy();
});
