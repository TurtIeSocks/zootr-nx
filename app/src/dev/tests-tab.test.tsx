import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { devApply, game, goal, goals, ok, saved, setup } from "@/test/dev-game";
import { fakeGame, openTab, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

test("a saved setup applies live, its lines in the entry", async () => {
  await renderReady();
  await openTab("Tests");
  fireEvent.click(screen.getByRole("button", { name: "Apply Now" }));
  expect(devApply).toHaveBeenCalledWith("Well");
  expect((await screen.findByRole("status")).textContent).toBe(
    "Applied Well\ngave Item_A\nsent to X/Y",
  );
});

test("a setup counts its met goals, marking ones held before Apply", async () => {
  saved.goals = ["Seen=true", "Lens=true", "Nope=true"];
  goals.Well = [
    goal("Seen", true),
    goal("Lens", false),
    goal("Nope", false, "no entry"),
  ];
  await renderReady();
  await openTab("Tests");
  expect((await screen.findByLabelText("Goals open")).textContent).toBe("1/3");
  expect(screen.getByText("(no entry)")).toBeTruthy();
  expect(screen.getByText("(now false)")).toBeTruthy();
  expect(screen.getByText("a lens")).toBeTruthy();

  devApply.mockReturnValueOnce(ok({ done: ["sent to X/Y"], held: ["Seen"] }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Now" }));
  expect((await screen.findByRole("status")).textContent).toBe(
    "Applied Well\nsent to X/Y\nSeen held before Apply",
  );
  saved.goals = ["Seen=true", "Lens=true"];
  goals.Well = [goal("Seen", true), goal("Lens", true)];
  const met = await screen.findByLabelText("Goals met", {}, { timeout: 2000 });
  expect(met.textContent).toBe("2/2");
  expect(met.className).toContain("bg-primary-soft");
  expect(screen.getAllByText("(held before Apply)")).toHaveLength(1);
});

test("a setup shows its goals from the config before the game answers", async () => {
  saved.goals = ["IsGet.Item_B=true"];
  game.state = undefined;
  render(<DevWindow />);
  await openTab("Tests");
  expect(await screen.findByLabelText("Goals open")).toBeTruthy();
  expect(screen.getByText(/IsGet.Item_B=true/)).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Apply Now" }).hasAttribute("disabled"),
  ).toBe(true);
});

test("a setup whose name holds a dot still shows its goals and holds", async () => {
  setup.name = "Well v1.2";
  saved.goals = ["Seen=true", "Lens=true"];
  goals[setup.name] = [goal("Seen", true), goal("Lens", false)];
  await renderReady();
  await openTab("Tests");
  expect(await screen.findByLabelText("Goals open")).toBeTruthy();
  expect(screen.getByText("Well v1.2")).toBeTruthy();
  expect(screen.getByText("a lens")).toBeTruthy();

  devApply.mockReturnValueOnce(ok({ done: [], held: ["Seen"] }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Now" }));
  expect(devApply).toHaveBeenCalledWith("Well v1.2");
  expect(await screen.findByText("(held before Apply)")).toBeTruthy();
});

test("a setup with no goals says what it applies", async () => {
  saved.expect = "";
  await renderReady();
  await openTab("Tests");
  expect(await screen.findByLabelText("No goals")).toBeTruthy();
  expect(screen.getByText(/No goals. Applies the spawn/)).toBeTruthy();
  saved.expect = "a lens";
});
