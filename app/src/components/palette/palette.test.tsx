import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { Box } from "lucide-react";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { Palette, type PaletteItem } from "@/components/palette/palette";

afterEach(cleanup);

const GROUPS = [
  { id: "a", title: "Alpha" },
  { id: "b", title: "Beta" },
];

const item = (id: string, group: string, extra: Partial<PaletteItem> = {}) =>
  ({ id, group, icon: Box, title: id, ...extra }) satisfies PaletteItem;

/** A palette over `items`, the query kept in state. */
// biome-ignore lint/style/useComponentExportOnlyModules: a test file never hot reloads
function Harness({
  items,
  onRun = () => {},
  onClose = () => {},
}: {
  items: PaletteItem[];
  onRun?: (i: PaletteItem) => void;
  onClose?: () => void;
}) {
  const [query, setQuery] = useState("");
  return (
    <Palette
      label="Search"
      placeholder=""
      hint="Type something"
      query={query}
      onQuery={setQuery}
      groups={GROUPS}
      items={items}
      cap={2}
      onRun={onRun}
      onClose={onClose}
    />
  );
}

const field = () => screen.getByRole("textbox", { name: "Search" });
const type = (q: string) => fireEvent.change(field(), { target: { value: q } });
const current = () =>
  screen
    .getAllByRole("button")
    .find((b) => b.getAttribute("aria-current") === "true")?.textContent;

test("says the hint with nothing typed, and No matches with nothing found", () => {
  render(<Harness items={[]} />);
  expect(screen.getByText("Type something")).toBeTruthy();
  type("zz");
  expect(screen.getByText("No matches")).toBeTruthy();
});

test("groups list in order, capped, counting the rest", () => {
  render(
    <Harness
      items={[
        item("b1", "b"),
        item("a1", "a"),
        item("a2", "a"),
        item("a3", "a"),
        item("a4", "a"),
      ]}
    />,
  );
  const alpha = within(screen.getByRole("list", { name: "Alpha" }));
  expect(alpha.getAllByRole("button").map((b) => b.textContent)).toEqual([
    "a1",
    "a2",
  ]);
  expect(screen.getByText("+2 more")).toBeTruthy();
  expect(screen.getAllByRole("heading").map((h) => h.textContent)).toEqual([
    "Alpha",
    "Beta",
  ]);
});

test("arrows move over the shown rows only; Enter runs; Esc closes", () => {
  const onRun = vi.fn();
  const onClose = vi.fn();
  render(
    <Harness
      items={[
        item("a1", "a"),
        item("a2", "a"),
        item("a3", "a"),
        item("b1", "b"),
      ]}
      onRun={onRun}
      onClose={onClose}
    />,
  );
  expect(current()).toBe("a1");
  fireEvent.keyDown(field(), { key: "ArrowDown" });
  fireEvent.keyDown(field(), { key: "ArrowDown" });
  expect(current()).toBe("b1");
  fireEvent.keyDown(field(), { key: "ArrowDown" });
  expect(current()).toBe("b1");
  fireEvent.keyDown(field(), { key: "Enter" });
  expect(onRun).toHaveBeenCalledWith(expect.objectContaining({ id: "b1" }));
  fireEvent.keyDown(field(), { key: "Escape" });
  expect(onClose).toHaveBeenCalled();
});

test("the arrows scroll the row they reach into view", () => {
  // jsdom has no scrollIntoView.
  const seen: string[] = [];
  HTMLElement.prototype.scrollIntoView = function (this: HTMLElement) {
    seen.push(this.textContent ?? "");
  };
  render(<Harness items={[item("a1", "a"), item("a2", "a")]} />);
  fireEvent.keyDown(field(), { key: "ArrowDown" });
  expect(seen.at(-1)).toBe("a2");
  // @ts-expect-error put jsdom back as it was
  delete HTMLElement.prototype.scrollIntoView;
});

test("hovering a row highlights it, and a new query starts at the top", () => {
  render(<Harness items={[item("a1", "a"), item("a2", "a")]} />);
  fireEvent.mouseMove(screen.getByRole("button", { name: "a2" }));
  expect(current()).toBe("a2");
  type("a");
  expect(current()).toBe("a1");
});

test("a row shows its trail, its title and its line, every word bolded", () => {
  render(
    <Harness
      items={[
        item("x", "a", {
          title: "Mod folder",
          trail: ["App", "Targets"],
          body: "Check this folder first",
        }),
      ]}
    />,
  );
  type("folder app");
  const row = screen.getByRole("button", { name: /Mod folder/ });
  expect(row.textContent).toBe(
    "App › Targets › Mod folderCheck this folder first",
  );
  expect(
    within(row)
      .getAllByText(/^(folder|App)$/i)
      .map((b) => b.tagName),
  ).toEqual(["B", "B", "B"]);
});
