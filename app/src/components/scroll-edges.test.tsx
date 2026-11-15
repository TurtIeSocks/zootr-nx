import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { ScrollEdges } from "@/components/scroll-edges";

afterEach(cleanup);

const MARKS = [
  { selector: "[data-glow]", label: "glowing", className: "bg-gold" },
];

/** A scroller `width` wide over content `content` wide, scrolled to `left`,
 * with a glowing card at `card` from the content's left edge. */
function scroller(width: number, content: number, card: number) {
  render(
    <ScrollEdges marks={MARKS}>
      <div>
        <button type="button" data-glow="all">
          Card
        </button>
      </div>
    </ScrollEdges>,
  );
  const glowing = screen.getByRole("button", { name: "Card" });
  const el = glowing.parentElement?.parentElement as HTMLElement;
  let left = 0;
  Object.defineProperties(el, {
    clientWidth: { get: () => width },
    scrollWidth: { get: () => content },
    scrollLeft: { get: () => left, set: (v: number) => (left = v) },
  });
  el.scrollBy = ((o: ScrollToOptions) => {
    left = Math.min(content - width, Math.max(0, left + (o.left ?? 0)));
    fireEvent.scroll(el);
  }) as typeof el.scrollBy;
  el.getBoundingClientRect = () => ({ left: 0, right: width }) as DOMRect;
  glowing.getBoundingClientRect = () =>
    ({ left: card - left, right: card - left + 200 }) as DOMRect;
  act(() => fireEvent.scroll(el));
}

const edge = (side: "left" | "right") =>
  screen.queryByRole("button", { name: new RegExp(`^Scroll ${side}`) });

test("no edges when everything fits", () => {
  scroller(800, 800, 0);
  expect(edge("left")).toBeNull();
  expect(edge("right")).toBeNull();
});

test("an edge shows on the side with more, and scrolls that way", () => {
  scroller(400, 1000, 0);
  expect(edge("left")).toBeNull();
  const right = edge("right");
  expect(right).toBeTruthy();
  act(() => fireEvent.click(right as HTMLElement));
  expect(edge("left")).toBeTruthy();
});

test("a marked card past an edge puts a dot on it", () => {
  scroller(400, 1600, 900);
  expect(edge("right")?.getAttribute("aria-label")).toBe(
    "Scroll right, glowing past here",
  );
  expect(edge("right")?.querySelector(".bg-gold")).toBeTruthy();
  act(() => fireEvent.click(edge("right") as HTMLElement));
  act(() => fireEvent.click(edge("right") as HTMLElement));
  // Two steps of 320 bring the card's middle into view: no dots.
  expect(edge("right")?.getAttribute("aria-label")).toBe("Scroll right");
  expect(edge("left")?.getAttribute("aria-label")).toBe("Scroll left");
});
