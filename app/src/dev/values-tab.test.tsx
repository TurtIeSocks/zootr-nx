import { fireEvent, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useUi } from "@/stores/ui";
import { devSet, devValue, err, values } from "@/test/dev-game";
import { fakeGame, openTab, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

/** Types a name into Values and waits for its field, which `ready` says
 * is the one for that name. */
async function pick(name: string, ready: (field: HTMLElement) => boolean) {
  const field = await screen.findByRole("textbox", { name: "GameData name" });
  fireEvent.change(field, { target: { value: name } });
  await vi.waitFor(() =>
    expect(ready(screen.getByLabelText("Value"))).toBe(true),
  );
  return screen.getByLabelText("Value") as HTMLInputElement & HTMLSelectElement;
}

const isSelect = (f: HTMLElement) => f.tagName === "SELECT";

/** The window on Values, the game read. */
async function onValues() {
  await renderReady();
  await openTab("Values");
}

test("picking a name shows its type, default and live value, in a field for its type", async () => {
  await onValues();
  const bool = await pick(
    "FakeHill_ArriveEntrance",
    (f) => f.getAttribute("role") === "switch",
  );
  expect(bool.getAttribute("aria-checked")).toBe("true");

  const step = await pick(
    "Step_FakeCastle",
    (f) =>
      f.tagName === "SELECT" && (f as HTMLSelectElement).options.length === 4,
  );
  expect(step.value).toBe("Step01");
  const card = screen.getByRole("region", { name: "GameData" });
  expect(within(card).getByText("Enum")).toBeTruthy();
  expect(within(card).getByText("NotReady", { selector: "dd" })).toBeTruthy();

  const rupees = await pick(
    "PlayerStatus.CurrentRupee",
    (f) => f.tagName === "INPUT",
  );
  expect(rupees.type).toBe("number");
  expect(rupees.value).toBe("42");

  const text = await pick(
    "FakeFlag_CurrentMap",
    (f) => (f as HTMLInputElement).type === "text",
  );
  expect(text.value).toBe("FakeHill_Child");

  const array = await pick("Fake_Array", (f) => f.tagName === "TEXTAREA");
  expect(array.value).toBe("[1, 2]");
  expect(array.hasAttribute("readonly")).toBe(true);
  expect(screen.queryByRole("button", { name: "Set" })).toBeNull();
});

test("the name field suggests names, Enter picking the first", async () => {
  await onValues();
  const name = screen.getByRole("textbox", { name: "GameData name" });
  fireEvent.change(name, { target: { value: "child" } });
  fireEvent.keyDown(name, { key: "Enter" });
  expect((name as HTMLInputElement).value).toBe("FakeFlag_IsChild");
});

test("without the game the field shows the default and says so", async () => {
  devValue.mockImplementationOnce(() =>
    Promise.resolve({
      status: "ok",
      data: { ...values.Step_FakeCastle, game: false, live: null },
    } as never),
  );
  await onValues();
  const step = await pick("Step_FakeCastle", isSelect);
  expect(step.value).toBe("NotReady");
  expect(screen.getByText(/isn't running, showing the default/)).toBeTruthy();
});

test("Set sends the picked value, reads it again, and heads Recent", async () => {
  await onValues();
  const step = await pick("Step_FakeCastle", isSelect);
  expect(step.value).toBe("Step01");
  fireEvent.change(step, { target: { value: "Complete" } });
  fireEvent.click(screen.getByRole("button", { name: "Set" }));
  expect(devSet).toHaveBeenCalledWith("Step_FakeCastle", "Complete");
  expect((await screen.findByRole("status")).textContent).toBe(
    "Step_FakeCastle = Complete",
  );
  await vi.waitFor(() => expect(devValue).toHaveBeenCalledTimes(2));
  const recent = screen.getByRole("list", { name: "Recent values" });
  expect(within(recent).getByText("Complete")).toBeTruthy();

  fireEvent.change(screen.getByRole("textbox", { name: "GameData name" }), {
    target: { value: "" },
  });
  fireEvent.click(within(recent).getByRole("button"));
  expect(useUi.getPath("dev.set.name")).toBe("Step_FakeCastle");
});

test("Read again reads the name again", async () => {
  await onValues();
  await pick("PlayerStatus.CurrentRupee", (f) => f.tagName === "INPUT");
  fireEvent.click(screen.getByRole("button", { name: "Read again" }));
  await vi.waitFor(() => expect(devValue).toHaveBeenCalledTimes(2));
});

test("a refused value shows the reason and an unknown name shows why", async () => {
  devSet.mockImplementationOnce(() => err("Step1 isn't a value of the name"));
  await onValues();
  await pick("PlayerStatus.CurrentRupee", (f) => f.tagName === "INPUT");
  fireEvent.click(screen.getByRole("button", { name: "Set" }));
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Couldn't set PlayerStatus.CurrentRupee: Step1 isn't a value of the name",
  );
  expect(useUi.getPath("dev.recent.values")).toEqual([]);
  fireEvent.change(screen.getByRole("textbox", { name: "GameData name" }), {
    target: { value: "0xdeadbeef" },
  });
  expect(await screen.findByText("no entry named 0xdeadbeef")).toBeTruthy();
});

test("a formula shows how it's worked out, its names loading each to set", async () => {
  await onValues();
  fireEvent.change(screen.getByRole("textbox", { name: "GameData name" }), {
    target: { value: "Flag_A_Exp" },
  });
  const card = await screen.findByRole("region", {
    name: "Worked out, not stored",
  });
  expect(within(card).getByText(/set those instead/)).toBeTruthy();
  expect(screen.queryByLabelText("Value")).toBeNull();
  expect(screen.queryByRole("button", { name: "Set" })).toBeNull();
  expect(screen.queryByText(/doesn't hold this value/)).toBeNull();
  // Not a name the game has: plain text.
  expect(within(card).queryByRole("button", { name: "Complete" })).toBeNull();

  fireEvent.click(within(card).getByRole("button", { name: "Step_A" }));
  expect(useUi.getPath("dev.set.name")).toBe("Step_A");
  await vi.waitFor(() =>
    expect((screen.getByLabelText("Value") as HTMLSelectElement).value).toBe(
      "Step01",
    ),
  );
  expect(
    screen.queryByRole("region", { name: "Worked out, not stored" }),
  ).toBeNull();
});

test("a saved step says it leaves the quest, and picks the quest in Give", async () => {
  await onValues();
  await pick("Step_A", isSelect);
  expect(await screen.findByText(/leaves the quest where it was/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Move the quest" }));
  expect(useUi.getPath("dev.quest")).toEqual({
    challenge: "A",
    step: "Step01",
  });
  expect(
    screen.getByRole("tab", { name: "Give" }).getAttribute("aria-selected"),
  ).toBe("true");
  expect(
    (screen.getByRole("textbox", { name: "Quest" }) as HTMLInputElement).value,
  ).toBe("A");
});
