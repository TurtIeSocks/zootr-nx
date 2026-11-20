import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { Help } from "@/components/help";
import { useUi } from "@/stores/ui";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const icon = () => screen.getByRole("button", { name: "About Control socket" });
const card = () => screen.queryByText("Control socket", { selector: "p" });
const wait = (ms: number) => act(() => vi.advanceTimersByTime(ms));

test("resting on the icon opens the card, which stays while it's hovered", async () => {
  render(<Help topic="control-socket" />);
  fireEvent.pointerEnter(icon());
  await wait(150);
  expect(card()).toBeNull();
  await wait(60);
  expect(card()).not.toBeNull();
  // Over to the card, for its link: it stays open.
  fireEvent.pointerLeave(icon());
  await wait(100);
  fireEvent.pointerEnter(card()?.parentElement as HTMLElement);
  await wait(500);
  expect(card()).not.toBeNull();
  fireEvent.pointerLeave(card()?.parentElement as HTMLElement);
  await wait(160);
  expect(card()).toBeNull();
});

test("a click pins it open until Escape, and a second click closes it", async () => {
  render(<Help topic="control-socket" />);
  fireEvent.click(icon());
  expect(card()).not.toBeNull();
  expect(document.activeElement).not.toBe(card()?.parentElement);
  fireEvent.pointerLeave(icon());
  await wait(500);
  expect(card()).not.toBeNull();
  fireEvent.keyDown(document.activeElement ?? document.body, {
    key: "Escape",
  });
  expect(card()).toBeNull();
  fireEvent.click(icon());
  fireEvent.click(icon());
  expect(card()).toBeNull();
});

test("its link opens the place it names and closes the card", () => {
  render(<Help topic="control-socket" />);
  fireEvent.click(icon());
  fireEvent.click(screen.getByRole("button", { name: "Open Live" }));
  expect(useUi.getState().tab).toBe("Live");
  expect(card()).toBeNull();
});
