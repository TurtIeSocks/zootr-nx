import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { InlineError } from "@/components/callout";

afterEach(cleanup);

test("an error with details shows its words, and the details on a click", () => {
  render(<InlineError error={"Choose the game\n\nopening a.nup: bad"} />);
  expect(screen.getByRole("alert").textContent).toContain("Choose the game");
  expect(screen.queryByText("opening a.nup: bad")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(screen.getByText("opening a.nup: bad")).toBeTruthy();
});

test("an error without details has no toggle", () => {
  render(<InlineError error="opening a.nup: bad" />);
  expect(screen.getByText("opening a.nup: bad")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});
