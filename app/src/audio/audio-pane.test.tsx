import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { AudioPane } from "@/audio/audio-pane";
import type { TrackView } from "@/bindings";
import { type FullConfig, full } from "@/lib/config";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

const setConfig = vi.fn((_c: FullConfig) =>
  Promise.resolve({ status: "ok", data: [] }),
);

const track = (
  id: string,
  label: string,
  pool: TrackView["pool"],
  song = true,
): TrackView => ({
  id,
  label,
  pool,
  band: 0,
  band_label: "-",
  ms: 60000,
  looping: true,
  song,
  gives_none: song ? null : "Its music is in sections",
});

const CATALOG = [
  track("kokiri-forest", "Kokiri Forest", "area"),
  track("market", "Market", "area", false),
  track("zora-domain", "Zora's Domain", "area"),
  track("enemy-battle", "Enemy battle", "battle"),
];

vi.mock("@/bindings", () => ({
  commands: {
    audioCatalog: async () => CATALOG,
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
  render(<AudioPane />);
}

const saved = () => setConfig.mock.lastCall?.[0];
const within = (label: string) =>
  screen.getByText(label).closest("li") as HTMLElement;
const press = (row: string, name: string) =>
  fireEvent.click(
    [...within(row).querySelectorAll("button")].find(
      (b) => b.textContent === name,
    ) as HTMLElement,
  );

test("lists a pool's tracks and saves the pool's mode", async () => {
  show(full({}));
  expect(await screen.findByText("Kokiri Forest")).toBeTruthy();
  expect(screen.queryByText("Enemy battle")).toBeNull();
  expect(screen.getByText("receives only")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Shuffle" }));
  await waitFor(() =>
    expect(saved()?.audio).toEqual({ pools: { area: "shuffle" } }),
  );
});

test("Pick gives a track its pool's first other song", async () => {
  show(full({}));
  await screen.findByText("Market");
  press("Market", "Pick");
  await waitFor(() =>
    expect(saved()?.audio).toEqual({ tracks: { market: "kokiri-forest" } }),
  );
});

test("Vanilla keeps a track out of its pool's shuffle", async () => {
  show(full({ audio: { pools: { area: "shuffle" } } }));
  await screen.findByText("Kokiri Forest");
  press("Kokiri Forest", "Vanilla");
  await waitFor(() =>
    expect(saved()?.audio).toEqual({
      pools: { area: "shuffle" },
      tracks: { "kokiri-forest": "vanilla" },
    }),
  );
});

test("Randomize shuffles every pool, Reset to Defaults clears", async () => {
  show(full({ audio: { tracks: { market: "zora-domain" } } }));
  await screen.findByText("Market");
  fireEvent.click(screen.getByRole("button", { name: "Randomize" }));
  await waitFor(() => expect(saved()?.audio?.pools?.fanfare).toBe("shuffle"));
  fireEvent.click(screen.getByRole("button", { name: "Reset to Defaults" }));
  await waitFor(() => expect(saved()?.audio).toEqual({}));
});

test("listening needs game assets", async () => {
  show(full({}));
  expect(await screen.findByText(/Turn on game assets/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Play / })).toBeNull();
});
