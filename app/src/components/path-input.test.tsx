import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { PathInput } from "@/components/path-input";

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (o: unknown) => open(o),
}));

afterEach(cleanup);

test("Browse fills the field from the picker", async () => {
  open.mockResolvedValue("/games/oot.nup");
  const onChange = vi.fn();
  render(<PathInput pick="file" value="" onChange={onChange} />);
  fireEvent.click(screen.getByRole("button", { name: /Browse/ }));
  await vi.waitFor(() =>
    expect(onChange).toHaveBeenCalledWith("/games/oot.nup"),
  );
});

test("a picker that fails says why", async () => {
  open.mockRejectedValue(new Error("No permission"));
  render(<PathInput pick="folder" value="" onChange={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: /Browse/ }));
  expect(await screen.findByText(/No permission/)).toBeTruthy();
});
