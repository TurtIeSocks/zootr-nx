import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { ErrorBoundary } from "@/components/error-boundary";

let broken = true;

// biome-ignore lint/style/useComponentExportOnlyModules: a test file never hot reloads
function Fragile() {
  if (broken) throw new Error("spots is null");
  return <p>Fine now</p>;
}

beforeEach(() => {
  broken = true;
  // React reports caught errors too; keep the output readable.
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

test("a view that throws shows what broke instead of a blank window", () => {
  render(
    <ErrorBoundary label="Spoiler">
      <Fragile />
    </ErrorBoundary>,
  );
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toContain("Spoiler hit a problem");
  expect(alert.textContent).toContain("spots is null");
});

test("Try again draws the view again", () => {
  render(
    <ErrorBoundary label="Spoiler">
      <Fragile />
    </ErrorBoundary>,
  );
  broken = false;
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(screen.getByText("Fine now")).toBeTruthy();
});

test("moving to another view clears the problem", () => {
  const { rerender } = render(
    <ErrorBoundary label="Checks" resetKey="Checks">
      <Fragile />
    </ErrorBoundary>,
  );
  expect(screen.getByRole("alert")).toBeTruthy();
  broken = false;
  rerender(
    <ErrorBoundary label="Items" resetKey="Items">
      <Fragile />
    </ErrorBoundary>,
  );
  expect(screen.getByText("Fine now")).toBeTruthy();
});

test("Copy details puts the error on the clipboard", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  render(
    <ErrorBoundary label="Spoiler">
      <Fragile />
    </ErrorBoundary>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
  expect(writeText).toHaveBeenCalledWith(
    expect.stringContaining("Spoiler: Error: spots is null"),
  );
});
