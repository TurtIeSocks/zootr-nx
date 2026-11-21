import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { useUi } from "@/stores/ui";
import { devGive, devStep, devValue } from "@/test/dev-game";
import { fakeGame, openTab, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

test("a name field suggests matches under itself, picked by click or Enter", async () => {
  await renderReady();
  await openTab("Give");
  const item = screen.getByRole("textbox", { name: "Item" });
  fireEvent.change(item, { target: { value: "item_a" } });
  const list = screen.getByRole("list", { name: "Item suggestions" });
  fireEvent.click(within(list).getByRole("button", { name: "Item_A" }));
  expect((item as HTMLInputElement).value).toBe("Item_A");
  expect(screen.queryByRole("list", { name: "Item suggestions" })).toBeNull();

  fireEvent.change(item, { target: { value: "bomb" } });
  fireEvent.keyDown(item, { key: "Enter" });
  expect((item as HTMLInputElement).value).toBe("Item_FakeBomb");
});

test("Give sends the item and count, the stepper keeping 1 to 99", async () => {
  await renderReady();
  await openTab("Give");
  const give = screen.getByRole("button", { name: "Give" });
  expect(give.hasAttribute("disabled")).toBe(true);
  fireEvent.change(screen.getByRole("textbox", { name: "Item" }), {
    target: { value: "Item_A" },
  });
  const count = screen.getByRole("spinbutton", { name: "Count" });
  expect(
    screen.getByRole("button", { name: "One fewer" }).hasAttribute("disabled"),
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "One more" }));
  expect((count as HTMLInputElement).value).toBe("2");
  fireEvent.change(count, { target: { value: "500" } });
  expect((count as HTMLInputElement).value).toBe("99");
  fireEvent.change(count, { target: { value: "20" } });
  fireEvent.click(give);
  expect(devGive).toHaveBeenCalledWith("Item_A", 20);
  expect((await screen.findByRole("status")).textContent).toBe(
    "gave Item_A ×20",
  );
});

test("a recent give repeats with its count, the last three kept", async () => {
  useUi.setPath("dev.recent.gives", [
    { item: "Item_A", count: 1 },
    { item: "Item_FakeBomb", count: 20 },
  ]);
  await renderReady();
  await openTab("Give");
  fireEvent.click(
    screen.getByRole("button", { name: "Give Item_FakeBomb ×20 again" }),
  );
  expect(devGive).toHaveBeenCalledWith("Item_FakeBomb", 20);
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.gives")).toEqual([
      { item: "Item_FakeBomb", count: 20 },
      { item: "Item_A", count: 1 },
    ]),
  );
  fireEvent.change(screen.getByRole("textbox", { name: "Item" }), {
    target: { value: "Item_FakeOther" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Give" }));
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.gives")).toHaveLength(3),
  );
  fireEvent.change(screen.getByRole("spinbutton", { name: "Count" }), {
    target: { value: "5" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Give" }));
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.gives")[0]).toEqual({
      item: "Item_FakeOther",
      count: 5,
    }),
  );
  expect(useUi.getPath("dev.recent.gives")).toHaveLength(3);
});

test("Quest Step moves the picked quest, and shows its saved copy before and after", async () => {
  await renderReady();
  await openTab("Give");
  const quest = screen.getByRole("textbox", { name: "Quest" });
  fireEvent.change(quest, { target: { value: "A" } });
  const step = (await screen.findByLabelText("Step")) as HTMLSelectElement;
  // Pinned, or a long step squeezes the quest field shut.
  expect(step.className).toContain("shrink-0");
  expect(step.value).toBe("NotReady");
  expect(await screen.findByText("Step_A = Step01")).toBeTruthy();
  expect(devValue).toHaveBeenCalledWith("Step_A");

  fireEvent.change(step, { target: { value: "Complete" } });
  fireEvent.click(screen.getByRole("button", { name: "Move" }));
  expect(devStep).toHaveBeenCalledWith("A", "Complete");
  expect((await screen.findByRole("status")).textContent).toBe(
    "A moved to Complete",
  );
  await vi.waitFor(() =>
    expect(devValue.mock.calls.filter(([n]) => n === "Step_A")).toHaveLength(2),
  );
});

test("Give's draft is kept in the view", async () => {
  await renderReady();
  await openTab("Give");
  fireEvent.change(screen.getByRole("textbox", { name: "Item" }), {
    target: { value: "Item_A" },
  });
  expect(useUi.getState().dev.give.item).toBe("Item_A");
  cleanup();
  render(<DevWindow />);
  await openTab("Give");
  expect(
    (screen.getByRole("textbox", { name: "Item" }) as HTMLInputElement).value,
  ).toBe("Item_A");
});
