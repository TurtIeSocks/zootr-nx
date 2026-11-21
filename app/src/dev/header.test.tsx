import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { useDev } from "@/stores/dev";
import {
  devAge,
  devSwitches,
  devTeleport,
  game,
  setAlwaysOnTop,
} from "@/test/dev-game";
import { fakeGame, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

/** The header's two lines: where Link is. */
const where = () => screen.getByRole("banner");

test("the place line shows the area, then the area and map id", async () => {
  await renderReady();
  expect(within(where()).getAllByText("FakeHill")).toHaveLength(2);
  expect(within(where()).getByText("FakeHill_Child")).toBeTruthy();
});

test("the place line shows the last warp's spot until the map changes", async () => {
  await renderReady();
  fireEvent.click(
    within(screen.getByRole("list", { name: "FakeHill" })).getByRole("button", {
      name: "FakeHill_FrontOfMarket",
    }),
  );
  await vi.waitFor(() => expect(devTeleport).toHaveBeenCalled());
  expect(
    await within(where()).findByText("FakeHill_FrontOfMarket"),
  ).toBeTruthy();

  act(() => useDev.setPath("game.state.map", "FakeKakariko_Child"));
  expect(within(where()).queryByText("FakeHill_FrontOfMarket")).toBeNull();
  expect(within(where()).getAllByText("FakeKakariko")).toHaveLength(2);
});

test("a loading map and a playing event show as a badge", async () => {
  await renderReady();
  act(() => useDev.setPath("game.state.loading", true));
  expect(within(where()).getByText("Loading")).toBeTruthy();
  act(() => {
    useDev.setPath("game.state.loading", false);
    useDev.setPath("game.state.event", true);
  });
  expect(within(where()).getByText("Event playing")).toBeTruthy();
});

test("the age picker shows Link's age and sends dev_age only for the other", async () => {
  await renderReady();
  const age = screen.getByRole("group", { name: "Age" });
  const child = within(age).getByRole("button", { name: "Child" });
  expect(child.getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(child);
  expect(devAge).not.toHaveBeenCalled();
  fireEvent.click(within(age).getByRole("button", { name: "Adult" }));
  expect(devAge).toHaveBeenCalledTimes(1);
  expect((await screen.findByRole("status")).textContent).toBe(
    "now adult, to FakeHill_Adult/X",
  );
});

test("the switch chips show what the game holds, and a toggle sets one", async () => {
  await renderReady();
  const health = screen.getByRole("button", { name: "Health" });
  const magic = screen.getByRole("button", { name: "Magic" });
  expect(health.getAttribute("aria-pressed")).toBe("true");
  expect(magic.getAttribute("aria-pressed")).toBe("false");
  expect(
    screen
      .getByRole("button", { name: "Resources" })
      .getAttribute("aria-pressed"),
  ).toBe("false");

  fireEvent.click(magic);
  expect(devSwitches).toHaveBeenCalledWith({
    health: true,
    magic: true,
    resources: false,
  });
  expect((await screen.findByRole("status")).textContent).toBe(
    "Infinite magic on",
  );
  expect(magic.getAttribute("aria-pressed")).toBe("true");
});

test("the pin takes the window off the top and back", async () => {
  await renderReady();
  fireEvent.click(screen.getByRole("button", { name: "Unpin from the top" }));
  await vi.waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalledWith(false));
  fireEvent.click(await screen.findByRole("button", { name: "Keep on top" }));
  await vi.waitFor(() => expect(setAlwaysOnTop).toHaveBeenCalledWith(true));
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

test("with the game gone, the line says so", async () => {
  game.state = undefined;
  render(<DevWindow />);
  expect(await within(where()).findByText("Game not running")).toBeTruthy();
});
