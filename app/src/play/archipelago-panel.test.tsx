import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { type ApStatus, commands, type Summary } from "@/bindings";
import { ArchipelagoPanel } from "@/play/archipelago-panel";
import { useAp } from "@/stores/ap";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

/** The window's file drops, as the panel last listened for them. */
const drops = vi.hoisted(() => ({
  send: (_: unknown) => {},
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (cb: (e: { payload: unknown }) => void) => {
      drops.send = (payload) => cb({ payload });
      return Promise.resolve(() => {});
    },
  }),
}));

const slot: Summary = {
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
  death_link: false,
};

/** Renders the folder with `installed`, the client at `status` having said
 * `log`, and the room kept as `server`. */
function show(
  status: ApStatus,
  log: string[] = [],
  { installed = slot as Summary | null, server = "" } = {},
) {
  useSeeds.setPath("status", { game: true, targets: [] });
  useSeeds.setPath("history", {
    installed,
    entries: installed ? [installed] : [],
  });
  useUi.setPath(
    "ap.rooms",
    server ? { [slot.id]: { server, password: "" } } : {},
  );
  useAp.setState({
    status,
    lines: log.map((text) => ({ time: 0, text })),
    error: null,
  });
  // Nothing stands in the way of an install.
  useSeeds.setState({ blocked: "", busy: false });
  render(<ArchipelagoPanel />);
}

const button = (name: string | RegExp) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;
const field = (name: string) => screen.getByLabelText(name) as HTMLInputElement;

afterEach(() => {
  cleanup();
  useUi.setPath("ap.rooms", {});
  vi.restoreAllMocks();
  clearMocks();
});

test("with no AP slot installed, the room asks for one", () => {
  show("offline", [], { installed: null });
  expect(
    screen.getByText("Install an Archipelago slot to connect to its room"),
  ).toBeTruthy();
  expect(screen.queryByLabelText("Server")).toBeNull();
});

test("no room yet points at the server, then connects to the one typed", async () => {
  const connect = vi
    .spyOn(commands, "apConnect")
    .mockResolvedValue({ status: "ok", data: null });
  show("offline");
  expect(screen.getByRole("status").textContent).toContain("No room yet");
  fireEvent.click(button("Set up room"));
  expect(document.activeElement).toBe(field("Server"));
  fireEvent.change(field("Server"), { target: { value: "localhost:38281" } });
  expect(useUi.getState().ap.rooms[slot.id]?.server).toBe("localhost:38281");
  expect(screen.getByRole("status").textContent).toContain("Not connected");
  fireEvent.click(button("Connect"));
  await waitFor(() =>
    expect(connect).toHaveBeenCalledWith("localhost:38281", null),
  );
});

test("connected, the fields lock and Disconnect leaves", () => {
  const leave = vi.spyOn(commands, "apDisconnect").mockResolvedValue();
  show("connected", [], {
    installed: { ...slot, death_link: true },
    server: "localhost:38281",
  });
  const status = screen.getByRole("status").textContent;
  expect(status).toContain("Connected as FakeSlot");
  expect(status).toContain("localhost:38281");
  expect(status).toContain("DeathLink");
  expect(field("Server").disabled).toBe(true);
  expect(field("Password").disabled).toBe(true);
  fireEvent.click(button("Disconnect"));
  expect(leave).toHaveBeenCalledOnce();
});

test("a lost connection reads Reconnecting, and Cancel stops it", () => {
  const leave = vi.spyOn(commands, "apDisconnect").mockResolvedValue();
  show("connecting", ["connected", "lost (closed), retrying in 5 s"], {
    server: "localhost:38281",
  });
  expect(screen.getByRole("status").textContent).toContain("Reconnecting");
  fireEvent.click(button("Cancel"));
  expect(leave).toHaveBeenCalledOnce();
});

test("a refusal reads as a sentence and Try again connects", async () => {
  const connect = vi
    .spyOn(commands, "apConnect")
    .mockResolvedValue({ status: "ok", data: null });
  show(
    "refused",
    ["refused: Archipelago refused connection: invalid or missing password"],
    { server: "localhost:38281" },
  );
  expect(screen.getByRole("status").textContent).toBe(
    "Refused: The password is wrong or missing.Try again",
  );
  fireEvent.click(button("Try again"));
  await waitFor(() => expect(connect).toHaveBeenCalledOnce());
});

test("a connect the client can't start says why", async () => {
  vi.spyOn(commands, "apConnect").mockResolvedValue({
    status: "error",
    error: "no seed is installed",
  });
  show("offline", [], { server: "x" });
  fireEvent.click(button("Connect"));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "no seed is installed",
  );
});

test("the password shows and hides", () => {
  show("offline");
  expect(field("Password").type).toBe("password");
  fireEvent.click(button("Show password"));
  expect(field("Password").type).toBe("text");
});

test("each log line gets its kind, and the filter keeps its own", () => {
  show("connected", [
    "connecting",
    "connected",
    "sent Fake Chest",
    "received Fake Sword from FakeFriend",
    "DeathLink: FakeFriend died",
    "goal sent",
    "lost (closed), retrying in 5 s",
    "something broke",
  ]);
  const kinds = () =>
    [...screen.getByRole("list", { name: "Client log" }).children].map(
      (li) => (li as HTMLElement).dataset.kind,
    );
  expect(kinds()).toEqual([
    "room",
    "room",
    "sent",
    "received",
    "death",
    "goal",
    "lost",
    "trouble",
  ]);
  fireEvent.click(button("Items"));
  expect(kinds()).toEqual(["sent", "received"]);
  fireEvent.click(button("Room"));
  expect(kinds()).not.toContain("sent");
  expect(kinds()).toHaveLength(6);
});

test("Copy log puts every line on the clipboard", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  show("connected", ["connected", "sent Fake Chest"]);
  fireEvent.click(button("Copy log"));
  await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
  expect(writeText.mock.calls[0]).toEqual([
    expect.stringMatching(/ connected\n.* sent Fake Chest$/),
  ]);
});

test("an .apznx dropped on the window installs that slot", async () => {
  mockIPC(() => {});
  const install = vi
    .spyOn(commands, "generateAp")
    .mockResolvedValue({ status: "ok", data: slot });
  show("offline");
  drops.send({ type: "enter", paths: ["/fake/FakeSlot.apznx"] });
  await waitFor(() =>
    expect(document.querySelector("[data-over=true]")).not.toBeNull(),
  );
  drops.send({
    type: "drop",
    paths: ["/fake/notes.txt", "/fake/FakeSlot.apznx"],
  });
  await waitFor(() =>
    expect(install).toHaveBeenCalledWith(
      "/fake/FakeSlot.apznx",
      expect.anything(),
    ),
  );
});
