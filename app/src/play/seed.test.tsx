import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { ExtrasBadges, Name } from "@/play/seed";
import { summary } from "@/test/fixtures";

afterEach(cleanup);

const entry = summary({
  label: { name: "Fake seed", note: "A fake note" },
  share: "ZR1-fake",
  test: true,
  plando: true,
});

test.each([
  ["Plando", "Built with a plando: items pinned to checks"],
  ["Test", "Built with a test setup: a spawn, items or new-game flags"],
])("the %s badge takes focus and opens its tip", async (label, tip) => {
  render(<ExtrasBadges entry={entry} />);
  const badge = screen.getByText(label);
  expect(badge.tabIndex).toBe(0);
  badge.focus();
  expect((await screen.findByRole("tooltip")).textContent).toBe(tip);
});

test("a seed's name takes focus and opens its note", async () => {
  render(<Name entry={entry} />);
  const name = screen.getByText("Fake seed");
  expect(name.tabIndex).toBe(0);
  name.focus();
  expect((await screen.findByRole("tooltip")).textContent).toContain(
    "A fake note",
  );
});
