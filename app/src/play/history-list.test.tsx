import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Summary } from "@/bindings";
import { full } from "@/lib/config";
import { drawTo, PAGE } from "@/play/history-filter";
import { HistoryList } from "@/play/history-list";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";
import { summary } from "@/test/fixtures";

const starSeeds = vi.fn(async (_ids: number[], _on: boolean) => ({
  status: "ok",
  data: null,
}));
const deleteSeeds = vi.fn(async (_ids: number[]) => ({
  status: "ok",
  data: 0,
}));

vi.mock("@/bindings", () => ({
  commands: {
    starSeeds: (ids: number[], on: boolean) => starSeeds(ids, on),
    deleteSeeds: (ids: number[]) => deleteSeeds(ids),
    status: async () => ({ status: "ok", data: { game: true, targets: [] } }),
    history: async () => ({ status: "ok", data: useSeeds.getState().history }),
  },
}));

afterEach(() => {
  cleanup();
  starSeeds.mockClear();
  deleteSeeds.mockClear();
  useShell.setState({ historyDrawn: undefined });
});

/** `n` seeds, newest first, named by id. */
const seeds = (
  n: number,
  patch: (id: number) => Partial<Summary> = () => ({}),
) =>
  Array.from({ length: n }, (_, i) => {
    const id = n - i;
    return summary({ id, label: { name: `Seed ${id}` }, ...patch(id) });
  });

function show(entries: Summary[], installed: Summary | null = null) {
  useConfig.setPath("config", full({}));
  useSeeds.setPath("history", { installed, entries });
  useUi.setPath("play", { tab: null, history: "all" });
  render(<HistoryList />);
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });
const rows = () => screen.getAllByRole("listitem");
const row = (name: string) =>
  rows().find((r) => within(r).queryByText(name)) as HTMLElement;

test("History draws a page of rows, then a page more each time", () => {
  show(seeds(PAGE + 30));
  expect(rows()).toHaveLength(PAGE);
  expect(screen.getByText(`${PAGE + 30} seeds`)).toBeTruthy();
  fireEvent.click(button("Show 30 more"));
  expect(rows()).toHaveLength(PAGE + 30);
  expect(screen.queryByRole("button", { name: /more$/ })).toBeNull();
});

test("Find draws far enough down to reveal a seed past the first page", () => {
  show(seeds(PAGE * 2 + 5));
  drawTo(3);
  expect(useShell.getState().historyDrawn).toBe(PAGE * 3);
  drawTo(PAGE * 2);
  // Never fewer than already drawn.
  expect(useShell.getState().historyDrawn).toBe(PAGE * 3);
});

test("the count names the starred seeds", () => {
  show(seeds(3, (id) => ({ starred: id === 2 })));
  expect(screen.getByText("3 seeds · 1 starred")).toBeTruthy();
});

test("select mode picks rows, shift-click picks a run, and stars them", async () => {
  show(seeds(5));
  expect(screen.queryByRole("checkbox")).toBeNull();
  fireEvent.click(button("Select"));
  expect(screen.queryByRole("button", { name: "Star seed" })).toBeNull();
  fireEvent.click(row("Seed 5"));
  fireEvent.click(row("Seed 3"), { shiftKey: true });
  expect(screen.getByText("3 selected")).toBeTruthy();
  fireEvent.click(button("Star"));
  await waitFor(() => expect(starSeeds).toHaveBeenCalledWith([5, 4, 3], true));
  fireEvent.click(button("Select all"));
  expect(screen.getByText("5 selected")).toBeTruthy();
  fireEvent.click(button("Done"));
  expect(screen.queryByRole("checkbox")).toBeNull();
  expect(button("Select")).toBeTruthy();
});

test("a key on a row's checkbox picks it, once", () => {
  show(seeds(2));
  fireEvent.click(button("Select"));
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Seed 1" }));
  expect(screen.getByText("1 selected")).toBeTruthy();
});

test("bulk delete counts the starred and leaves the installed seed", async () => {
  const entries = seeds(3, (id) => ({ starred: id === 3 }));
  show(entries, entries[2] ?? null);
  fireEvent.click(button("Select"));
  fireEvent.click(button("Select all"));
  fireEvent.click(button("Delete 2"));
  expect(button("Delete 2, 1 starred? Installed stays")).toBeTruthy();
  fireEvent.click(button("Delete 2, 1 starred? Installed stays"));
  await waitFor(() => expect(deleteSeeds).toHaveBeenCalledWith([3, 2]));
});

test("only the installed seed picked, Delete stays off", () => {
  const entries = seeds(2);
  show(entries, entries[1] ?? null);
  fireEvent.click(button("Select"));
  fireEvent.click(row("Seed 1"));
  expect((button("Delete 0") as HTMLButtonElement).disabled).toBe(true);
});
