import { act, fireEvent, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";
import { fakeGame, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

test("the saved tab opens", async () => {
  useUi.setPath("dev.tab", "values");
  await renderReady();
  expect(
    screen.getByRole("tab", { name: "Values" }).getAttribute("aria-selected"),
  ).toBe("true");
  expect(screen.getByRole("region", { name: "GameData" })).toBeTruthy();
});

test("Actors shows how many actors are hidden", async () => {
  await renderReady();
  expect(screen.getByRole("tab", { name: "Actors" })).toBeTruthy();
  act(() =>
    useDev.setPath("live.hidden", [
      { hash: "1", actor: "Obj_FakeRock", label: null, map: "FakeHill_Child" },
      { hash: "2", actor: "Obj_FakeTree", label: null, map: "FakeHill_Child" },
    ]),
  );
  expect(screen.getByRole("tab", { name: "Actors, 2 hidden" })).toBeTruthy();
});

test("arrow keys move along the tabs", async () => {
  await renderReady();
  const warp = screen.getByRole("tab", { name: "Warp" });
  fireEvent.keyDown(warp, { key: "ArrowLeft" });
  expect(useUi.getPath("dev.tab")).toBe("tests");
  fireEvent.keyDown(warp, { key: "ArrowRight" });
  expect(useUi.getPath("dev.tab")).toBe("warp");
});
