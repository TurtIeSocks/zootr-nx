import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands } from "@/bindings";
import { RoomStatus } from "@/play/room-status";
import { useAp } from "@/stores/ap";
import { useLive } from "@/stores/live";
import { useUi } from "@/stores/ui";
import { summary } from "@/test/fixtures";

const slot = summary({ id: 3, seed: "1", ap_slot: "FakeSlot" });

afterEach(() => {
  cleanup();
  useUi.setPath("ap.rooms", {});
  vi.restoreAllMocks();
});

test("a Connect that can't start says why beside its button, with no panel open", async () => {
  useLive.setPath("status", { state: "Attaching", reason: "" });
  useUi.setPath("ap.rooms", { [slot.id]: { server: "host:1", password: "" } });
  useAp.setState({ status: "offline", lines: [], error: null });
  vi.spyOn(commands, "apConnect").mockResolvedValue({
    status: "error",
    error: "The game isn't running",
  });
  render(<RoomStatus entry={slot} />);
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "The game isn't running",
  );
});
