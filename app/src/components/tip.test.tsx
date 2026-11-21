import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { Tip } from "@/components/tip";
import { SOURCES } from "@/test/sources";

afterEach(cleanup);

/** Components whose `title` prop is their own heading, not a hover hint. */
const OWN_TITLE = new Set([
  "Card",
  "CardHeader",
  "Checklist",
  "DevCard",
  "Fold",
  "GroupHead",
  "NavGroup",
  "PageHeader",
  "PlaceGroup",
  "Rows",
  "Setting",
  "Subgroup",
]);

/** The tag of the JSX element whose attributes hold `at`. */
function tagAt(text: string, at: number): string {
  let depth = 0;
  for (let i = at - 1; i >= 0; i--) {
    const c = text[i];
    if (c === "}") depth++;
    else if (c === "{") depth--;
    else if (c === "<" && depth === 0 && /[A-Za-z]/.test(text[i + 1] ?? ""))
      return /^<([\w.]+)/.exec(text.slice(i))?.[1] ?? "";
  }
  return "";
}

test("no native title is left: every hover hint is a Tip", () => {
  expect(Object.keys(SOURCES).length).toBeGreaterThan(100);
  const fake = '<td className={cn(a, { b: c })} title="x">';
  expect(tagAt(fake, fake.indexOf(" title="))).toBe("td");
  const titled = Object.entries(SOURCES)
    .filter(([path]) => path.endsWith(".tsx"))
    .flatMap(([path, text]) =>
      [...text.matchAll(/\stitle=/g)].map((m) => ({
        path,
        tag: tagAt(text, m.index),
      })),
    )
    .filter(({ tag }) => !OWN_TITLE.has(tag));
  expect(titled).toEqual([]);
});

test("a tip shows on focus and says what it was given", async () => {
  render(
    <Tip tip="Forest Temple">
      <span>FoT</span>
    </Tip>,
  );
  fireEvent.focus(screen.getByText("FoT"));
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    "Forest Temple",
  );
});

test.each(["", false])("an empty tip (%j) leaves the child alone", (tip) => {
  render(
    <Tip tip={tip}>
      <span>FoT</span>
    </Tip>,
  );
  fireEvent.focus(screen.getByText("FoT"));
  expect(screen.getByText("FoT").getAttribute("data-state")).toBeNull();
});
