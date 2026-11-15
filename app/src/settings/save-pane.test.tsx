import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { SavePane } from "@/settings/save-pane";

const editSave = vi.fn((_slot: number, _presets: string[]) =>
  Promise.resolve({
    status: "ok",
    data: { log: ["Flag_A: false -> true"], backup: "progress.sav.1.bak" },
  }),
);

vi.mock("@/bindings", () => ({
  commands: {
    saveOptions: () =>
      Promise.resolve({
        status: "ok",
        data: {
          slots: [0, 2],
          presets: [
            { name: "zelda-done", label: "Zelda met", what: "Lullaby" },
            { name: "trials-done", label: "Trials", what: "All six" },
          ],
        },
      }),
    editSave: (slot: number, presets: string[]) => editSave(slot, presets),
  },
}));

afterEach(cleanup);

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

test("applies the picked presets to the picked slot and shows the log", async () => {
  render(<SavePane />);
  await waitFor(() => expect(button("Slot 2")).toBeTruthy());
  expect(button("Apply").disabled).toBe(true);
  fireEvent.click(button("Slot 2"));
  fireEvent.click(screen.getByLabelText("Zelda met"));
  fireEvent.click(button("Apply"));
  await waitFor(() =>
    expect(screen.getByText("Flag_A: false -> true")).toBeTruthy(),
  );
  expect(editSave).toHaveBeenCalledWith(2, ["zelda-done"]);
  expect(screen.getByText("Backup at progress.sav.1.bak")).toBeTruthy();
});
