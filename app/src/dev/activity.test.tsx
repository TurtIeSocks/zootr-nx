import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { send, useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";
import * as fake from "@/test/dev-game";
import { fakeGame, openTab, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

test("a result lands in the line with how long ago, and a failure shows as one", async () => {
  await renderReady();
  expect(screen.getByText(/Results of what you send/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Magic" }));
  expect((await screen.findByRole("status")).textContent).toBe(
    "Infinite magic on",
  );
  expect(screen.getByText("now")).toBeTruthy();

  fake.devSwitches.mockImplementationOnce(() => fake.err("the hook is off"));
  fireEvent.click(screen.getByRole("button", { name: "Resources" }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe(
    "Couldn't turn infinite resources on: the hook is off",
  );
  expect(alert.parentElement?.className).toContain("bg-danger-soft");
});

test("the history holds the last 20, newest first, a line per line of each", async () => {
  for (let i = 0; i < 25; i++)
    await send("set", `set ${i}`, async () => `result ${i}`);
  await send("apply", "apply Well", async () => "Applied Well\nsent to X/Y");
  render(<DevWindow />);
  fireEvent.click(
    screen.getByRole("button", { name: "Show the last results" }),
  );
  const items = within(
    screen.getByRole("list", { name: "Last results" }),
  ).getAllByRole("listitem");
  expect(items).toHaveLength(20);
  expect(items[0]?.textContent).toMatch(/^Applied Well\nsent to X\/Y/);
  expect(items[1]?.textContent).toMatch(/^result 24/);
  expect(items[19]?.textContent).toMatch(/^result 6/);
});

test("a running request shows in the line and holds every other request back", async () => {
  let done = (_: unknown) => {};
  fake.devTeleport.mockImplementationOnce(
    () => new Promise((resolve) => (done = resolve)) as never,
  );
  await renderReady();
  const spot = within(screen.getByRole("list", { name: "FakeHill" })).getByRole(
    "button",
    { name: "FakeHill_FrontOfMarket" },
  );
  fireEvent.click(spot);
  expect(
    await screen.findByText("warp to FakeHill_FrontOfMarket…"),
  ).toBeTruthy();
  expect(spot.hasAttribute("disabled")).toBe(true);
  expect(
    screen.getByRole("button", { name: "Health" }).hasAttribute("disabled"),
  ).toBe(true);
  useUi.setPath("dev.give.item", "Item_A");
  await openTab("Give");
  expect(
    screen.getByRole("button", { name: "Give" }).hasAttribute("disabled"),
  ).toBe(true);

  await act(async () => done({ status: "ok", data: "sent" }));
  expect((await screen.findByRole("status")).textContent).toBe("sent");
  expect(
    screen.getByRole("button", { name: "Give" }).hasAttribute("disabled"),
  ).toBe(false);
});

test("a second request while one runs is dropped", async () => {
  let done = (_: string) => {};
  const first = send(
    "set",
    "set A",
    () => new Promise<string>((resolve) => (done = resolve)),
  );
  expect(await send("set", "set B", async () => "B")).toBe(false);
  done("A");
  expect(await first).toBe(true);
  expect(useDev.getPath("activity.log").map((e) => e.text)).toEqual(["A"]);
});
