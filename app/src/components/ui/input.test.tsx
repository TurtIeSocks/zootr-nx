import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test } from "vitest";
import { Input, type InputProps } from "@/components/ui/input";

afterEach(cleanup);

/** A controlled field, as the app's are, and what it last heard. */
function Field(props: InputProps & { start?: string }) {
  const [value, setValue] = useState(props.start ?? "");
  return (
    <>
      <Input
        aria-label="Field"
        {...props}
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
      <output>{value}</output>
    </>
  );
}

const field = () => screen.getByLabelText<HTMLInputElement>("Field");
const clearButton = () => screen.queryByRole("button", { name: "Clear" });

test("the × shows with text, empties the field through onChange and refocuses it", () => {
  render(<Field />);
  expect(clearButton()).toBeNull();
  fireEvent.change(field(), { target: { value: "Deku" } });
  fireEvent.click(clearButton() as HTMLElement);
  expect(field().value).toBe("");
  expect(screen.getByRole("status").textContent).toBe("");
  expect(document.activeElement).toBe(field());
  expect(clearButton()).toBeNull();
});

test("a number field, an opted-out field and a disabled one show no ×", () => {
  const { unmount } = render(<Field type="number" start="3" />);
  expect(clearButton()).toBeNull();
  unmount();
  render(<Field clearable={false} start="Deku" />);
  expect(clearButton()).toBeNull();
  cleanup();
  render(<Field disabled start="Deku" />);
  expect(clearButton()).toBeNull();
});
