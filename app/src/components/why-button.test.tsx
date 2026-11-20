import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { WhyButton } from "@/components/why-button";

afterEach(cleanup);

/** Renders a Go button that can't act for `why`. */
function show(why: string | false, onWhy?: () => void) {
  const onClick = vi.fn();
  render(
    <TooltipProvider>
      <WhyButton why={why} onWhy={onWhy} onClick={onClick}>
        Go
      </WhyButton>
    </TooltipProvider>,
  );
  return { onClick, button: screen.getByRole("button", { name: "Go" }) };
}

test("a reason holds the click and stays hoverable", () => {
  const onWhy = vi.fn();
  const { onClick, button } = show("FakeReason", onWhy);
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect((button as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(button);
  expect(onClick).not.toHaveBeenCalled();
  expect(onWhy).toHaveBeenCalledOnce();
});

test("with no reason it acts", () => {
  const { onClick, button } = show(false);
  expect(button.getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(button);
  expect(onClick).toHaveBeenCalledOnce();
});
