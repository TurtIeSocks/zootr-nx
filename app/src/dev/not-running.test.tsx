import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { send, useDev } from "@/stores/dev";
import * as fake from "@/test/dev-game";
import { fakeGame, openTab } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

/** The window with the game not running. */
async function stopped() {
  fake.game.state = undefined;
  render(<DevWindow />);
  return screen.findByText("Start the game to use the dev panel");
}

test("says how to start the game, its controls disabled", async () => {
  await stopped();
  expect(screen.getByText("Game not running")).toBeTruthy();
  for (const name of ["Health", "Magic", "Resources", "Child", "Adult"])
    expect(screen.getByRole("button", { name }).hasAttribute("disabled")).toBe(
      true,
    );
});

test("Play starts the picked target", async () => {
  await stopped();
  fireEvent.click(
    await screen.findByRole("button", { name: "Play FakeNative" }),
  );
  await vi.waitFor(() => expect(fake.play).toHaveBeenCalledWith("FakeNative"));
  expect((await screen.findByRole("status")).textContent).toBe(
    "Started FakeNative",
  );
});

test("Play says why it waits while a request runs", async () => {
  await stopped();
  act(() =>
    useDev.setPath("activity.running", { kind: "give", what: "give Item_A" }),
  );
  const button = screen.getByRole("button", { name: "Play FakeNative" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  fireEvent.focus(button);
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    "Wait for give Item_A to finish",
  );
  fireEvent.click(button);
  expect(fake.play).not.toHaveBeenCalled();
  act(() => useDev.setPath("activity.running", null));
});

test("Play leaves a stale build to the main window", async () => {
  fake.build.kind = "stale";
  await stopped();
  fireEvent.click(
    await screen.findByRole("button", { name: "Play FakeNative" }),
  );
  expect((await screen.findByRole("alert")).textContent).toMatch(
    /^Couldn't play FakeNative: its build lacks the installed hook patch/,
  );
  expect(fake.play).not.toHaveBeenCalled();
});

test("a tab still shows its list, its requests disabled, and the last result stays", async () => {
  await send("give", "give Item_A", async () => "gave Item_A ×1");
  await stopped();
  expect(screen.getByRole("status").textContent).toBe("gave Item_A ×1");
  await openTab("Warp");
  const field = await screen.findByRole("list", { name: "FakeHill" });
  for (const b of within(field).getAllByRole("button", { name: /^FakeHill/ }))
    expect(b.hasAttribute("disabled")).toBe(true);
  expect(screen.queryByText("Start the game to use the dev panel")).toBeNull();
});
