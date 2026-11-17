import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands, events, type Summary } from "@/bindings";
import { ApClient } from "@/play/ap-client";
import { stop, useAp } from "@/stores/ap";
import { useUi } from "@/stores/ui";

const seed: Summary = {
  id: 3,
  time: 0,
  seed: "1",
  icons: [],
  share: "",
  settings: {},
  label: {},
  chests: 0,
  changed: 0,
  test: false,
  plando: false,
  ap_slot: "FakeSlot",
};

function show(status: "offline" | "connected", log: string[] = []) {
  vi.spyOn(events.apEvent, "listen").mockResolvedValue(() => {});
  vi.spyOn(commands, "apState").mockResolvedValue({ status, log });
  render(<ApClient entry={seed} />);
}

afterEach(() => {
  cleanup();
  stop();
  useAp.setState({ status: "offline", log: [] });
  useUi.setPath("ap.rooms", {});
  vi.restoreAllMocks();
});

test("connects to the server typed, kept for this seed", async () => {
  const connect = vi
    .spyOn(commands, "apConnect")
    .mockResolvedValue({ status: "ok", data: null });
  show("offline");
  const button = screen.getByRole("button", { name: "Connect" });
  expect((button as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("Server"), {
    target: { value: "localhost:38281" },
  });
  expect(useUi.getState().ap.rooms[seed.id]?.server).toBe("localhost:38281");
  fireEvent.click(button);
  await waitFor(() =>
    expect(connect).toHaveBeenCalledWith("localhost:38281", null),
  );
});

test("a running client offers Disconnect and shows its log", async () => {
  show("connected", ["sent Fake Chest"]);
  await waitFor(() => expect(screen.getByText("sent Fake Chest")).toBeTruthy());
  expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
  expect((screen.getByLabelText("Server") as HTMLInputElement).disabled).toBe(
    true,
  );
  expect(screen.getByText("FakeSlot")).toBeTruthy();
});

test("a refused connect shows why", async () => {
  vi.spyOn(commands, "apConnect").mockResolvedValue({
    status: "error",
    error: "no seed is installed",
  });
  show("offline");
  fireEvent.change(screen.getByLabelText("Server"), {
    target: { value: "x" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() =>
    expect(screen.getByText("no seed is installed")).toBeTruthy(),
  );
});
