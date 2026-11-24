import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { Marked } from "@/components/palette/marked";

afterEach(cleanup);

test("every match of a word is bold", () => {
  const { container } = render(<Marked text="Deku Deku" words={["deku"]} />);
  const bold = [...container.querySelectorAll("b")].map((b) => b.textContent);
  expect(bold).toEqual(["Deku", "Deku"]);
});

test("an empty word bolds nothing and still returns", () => {
  const { container } = render(
    <Marked text="Deku Tree" words={["", "tree"]} />,
  );
  const bold = [...container.querySelectorAll("b")].map((b) => b.textContent);
  expect(bold).toEqual(["Tree"]);
});
