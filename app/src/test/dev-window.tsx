import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { useDev } from "@/stores/dev";
import { resetGame } from "@/test/dev-game";

/** Starts each test with the game running, and cleans up after. */
export function fakeGame() {
  beforeEach(resetGame);
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });
}

/** Renders the dev window and waits until requests can go and the
 * options have loaded. */
export async function renderReady() {
  render(<DevWindow />);
  await vi.waitFor(() => expect(useDev.getState().ready).toBe(true));
  await vi.waitFor(() =>
    expect(useDev.getState().options.places).not.toHaveLength(0),
  );
}

/** Opens a tab by its label. */
export const openTab = async (label: RegExp | string) =>
  fireEvent.click(await screen.findByRole("tab", { name: label }));

/** The buttons of the list labelled `name`. */
export const buttonsIn = (name: string) =>
  within(screen.getByRole("list", { name })).getAllByRole("button");
