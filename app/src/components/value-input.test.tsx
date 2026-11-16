import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { ValueInput } from "@/components/value-input";

afterEach(cleanup);

const info = (kind: string, input: "vector" | "choice") => ({
  kind,
  input,
  choices: input === "choice" ? ["a", "b"] : [],
  default: null,
});

test("a vector is a number per axis, joined with commas", () => {
  const onChange = vi.fn();
  render(
    <ValueInput
      info={info("Vector3", "vector")}
      value="1,2,3"
      onChange={onChange}
    />,
  );
  const y = screen.getByLabelText("Value y") as HTMLInputElement;
  expect(y.value).toBe("2");
  fireEvent.change(y, { target: { value: "-4.5" } });
  expect(onChange).toHaveBeenCalledWith("1,-4.5,3");
  expect(screen.queryByLabelText("Value z")).toBeTruthy();
});

test("a choice reads as its label where it has one", () => {
  render(
    <ValueInput
      info={info("String64", "choice")}
      value="a"
      labels={{ a: "Kokiri Sword" }}
      onChange={() => {}}
    />,
  );
  const options = [
    ...(screen.getByLabelText("Value") as HTMLSelectElement).options,
  ];
  expect(options.map((o) => o.textContent)).toEqual(["Kokiri Sword", "b"]);
});
