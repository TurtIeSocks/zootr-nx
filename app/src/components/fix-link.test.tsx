import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { FixLink } from "@/components/fix-link";
import { NEED_TARGET } from "@/lib/config";
import { useUi } from "@/stores/ui";

afterEach(cleanup);

test("a reason a page fixes links there, and any other is plain text", () => {
  useUi.setPath("tab", "Play");
  render(<FixLink why={NEED_TARGET} />);
  fireEvent.click(screen.getByRole("button", { name: NEED_TARGET }));
  expect(useUi.getState().tab).toBe("Settings");
  expect(useUi.getState().section).toBe("targets");
  cleanup();
  render(<FixLink why="Close the game first" />);
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.getByText("Close the game first")).toBeTruthy();
});
