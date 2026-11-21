import { fireEvent, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";
import * as fake from "@/test/dev-game";
import { fakeGame, openTab, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

/** Opens Find anything with ⌘K and types `query`. */
async function find(query: string) {
  fireEvent.keyDown(window, { key: "k", metaKey: true });
  const field = await screen.findByRole("textbox", { name: "Find anything" });
  fireEvent.change(field, { target: { value: query } });
  return field;
}

const dialog = () => screen.getByRole("dialog", { name: "Find anything" });
const group = (name: string) =>
  within(within(dialog()).getByRole("list", { name })).getAllByRole("button");

test("⌘K opens it, and the search button too; Esc closes it", async () => {
  await renderReady();
  const field = await find("");
  expect(dialog()).toBeTruthy();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Find anything" }));
  expect(await screen.findByRole("dialog")).toBeTruthy();
});

test("results group by kind, at most four each, the match bolded", async () => {
  fake.setup.name = "FakeWell";
  await renderReady();
  await find("fake");
  expect(group("Warp").map((b) => b.textContent)).toEqual([
    "FakeHill · FakeHill_FrontOfMarket",
    "FakeKakariko · FakeKakariko_FrontOfGate",
  ]);
  expect(group("Give").map((b) => b.textContent)).toEqual(["Item_FakeBomb"]);
  expect(group("Values")).toHaveLength(4);
  await vi.waitFor(() =>
    expect(group("Tests").map((b) => b.textContent)).toEqual(["FakeWell"]),
  );
  expect(
    within(group("Give")[0] as HTMLElement).getByText("Fake").tagName,
  ).toBe("B");
});

test("Enter runs the highlighted result of each kind", async () => {
  await renderReady();
  let field = await find("gate");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(fake.devTeleport).toHaveBeenCalledWith(
    "FakeKakariko_Child/FakeKakariko_FrontOfGate",
  );
  expect(screen.queryByRole("dialog")).toBeNull();
  await screen.findByRole("status");

  field = await find("bomb");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(fake.devGive).toHaveBeenCalledWith("Item_FakeBomb", 1);
  await vi.waitFor(() => expect(useDev.getPath("activity.running")).toBeNull());

  field = await find("well");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(fake.devApply).toHaveBeenCalledWith("Well");
  await vi.waitFor(() => expect(useDev.getPath("activity.running")).toBeNull());

  field = await find("rupee");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(useUi.getPath("dev.set.name")).toBe("PlayerStatus.CurrentRupee");
  expect(
    screen.getByRole("tab", { name: "Values" }).getAttribute("aria-selected"),
  ).toBe("true");
});

test("arrow keys move the highlight", async () => {
  await renderReady();
  const field = await find("fake");
  fireEvent.keyDown(field, { key: "ArrowDown" });
  fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(
    within(dialog())
      .getAllByRole("button")
      .find((b) => b.getAttribute("aria-current") === "true")?.textContent,
  ).toBe("Item_FakeBomb");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(fake.devGive).toHaveBeenCalledWith("Item_FakeBomb", 1);
});

test("Tab opens the result's tab with the query in its search", async () => {
  await renderReady();
  await openTab("Tests");
  let field = await find("gate");
  fireEvent.keyDown(field, { key: "Tab" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fake.devTeleport).not.toHaveBeenCalled();
  expect(
    (screen.getByRole("textbox", { name: "Search places" }) as HTMLInputElement)
      .value,
  ).toBe("gate");

  field = await find("bomb");
  fireEvent.keyDown(field, { key: "Tab" });
  expect(
    (screen.getByRole("textbox", { name: "Item" }) as HTMLInputElement).value,
  ).toBe("bomb");
  expect(fake.devGive).not.toHaveBeenCalled();
});
