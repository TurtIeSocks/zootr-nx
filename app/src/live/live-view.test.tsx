import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Event, Numbered, Session, Snapshot } from "@/bindings";
import { LiveView } from "@/live/live-view";
import { init } from "@/stores/live";
import { useUi } from "@/stores/ui";

const session: Session = {
  source: "Live",
  reason: null,
  log: "/l.log",
  unsaved: null,
  started: "22:51",
};

/** A stand-in for the backend: numbers events and sends them on. */
const backend = {
  listener: undefined as ((e: { payload: Numbered }) => void) | undefined,
  next: 0,
  snapshot: undefined as Snapshot | undefined,
  /** Why the next snapshot fails, if it does. */
  fail: undefined as string | undefined,
  /** Why revealing the log fails, if it does. */
  revealFail: undefined as string | undefined,
  send(event: Event) {
    const n = { seq: backend.next++, event };
    act(() => backend.listener?.({ payload: n }));
    return n;
  },
};

const ok = () => Promise.resolve({ status: "ok", data: null });
const liveMark = vi.fn((_what: string) => ok());
const liveMute = vi.fn((_hash: number) => ok());

const liveLabel = vi.fn((_hash: number, _label: string) =>
  Promise.resolve({ status: "ok", data: null }),
);

vi.mock("@/bindings", () => ({
  events: {
    liveEvent: {
      listen: (cb: (e: { payload: Numbered }) => void) => {
        backend.listener = cb;
        return Promise.resolve(() => {});
      },
    },
  },
  commands: {
    liveState: () =>
      backend.fail
        ? Promise.reject(new Error(backend.fail))
        : Promise.resolve(
            backend.snapshot ?? {
              status: { state: "Waiting", reason: null },
              events: [],
              dropped: 0,
              next: backend.next,
            },
          ),
    liveLabel: (hash: number, label: string) => liveLabel(hash, label),
    liveMark: (what: string) => liveMark(what),
    liveMute: (hash: number) => liveMute(hash),
    revealLog: () =>
      Promise.resolve(
        backend.revealFail
          ? { status: "error", error: backend.revealFail }
          : { status: "ok", data: null },
      ),
  },
}));

const writeText = vi.fn((_text: string) => Promise.resolve());
Object.assign(navigator, { clipboard: { writeText } });

afterEach(() => {
  cleanup();
  writeText.mockClear();
  useUi.setPath("live.fold", true);
  useUi.setPath("live.hiddenKinds", []);
  localStorage.clear();
  liveLabel.mockClear();
  liveMark.mockClear();
  liveMute.mockClear();
  backend.snapshot = undefined;
  backend.fail = undefined;
  backend.revealFail = undefined;
});

const send = (hash: number, name: string, label: string | null = null) =>
  backend.send({
    type: "Row",
    at: 75,
    save: false,
    change: { hash, name, label, kind: "Int", change: "1 -> 2" },
  });

/** Renders once the page has its snapshot. */
async function loaded() {
  await act(init);
  render(<LiveView />);
}

/** The game starts and the watcher records it. */
async function recording() {
  await loaded();
  backend.send({ type: "Started", session });
  backend.send({ type: "Status", status: { state: "Recording", session } });
  send(1, "PlayerStatus.CurrentRupee");
  send(0xe573f564, "#e573f564");
}

test("with no game it says what it's waiting for", async () => {
  await loaded();
  expect(screen.getByText("Waiting for the game")).toBeTruthy();
  expect(screen.getByText("Recording starts when the game does")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Record|Auto|Stop/ })).toBeNull();
});

test("rows show their time, and Quiet hides unnamed values", async () => {
  await recording();
  expect(screen.queryByText("Autosaves")).toBeNull();
  expect(screen.getByText("PlayerStatus.CurrentRupee")).toBeTruthy();
  expect(screen.getAllByText("01:15")).toHaveLength(1);
  expect(screen.queryByText("#e573f564")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  expect(screen.getByText("#e573f564")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Filter lines"), {
    target: { value: "rupee" },
  });
  expect(screen.queryByText("#e573f564")).toBeNull();
});

test("an unnamed row takes a label: Enter saves, Esc cancels", async () => {
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  fireEvent.click(screen.getByRole("button", { name: /^#e573f564/ }));
  const input = screen.getByLabelText("Label");
  fireEvent.change(input, { target: { value: "gate" } });
  fireEvent.keyDown(input, { key: "Escape" });
  expect(liveLabel).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: /^#e573f564/ }));
  fireEvent.change(screen.getByLabelText("Label"), {
    target: { value: "gate" },
  });
  fireEvent.keyDown(screen.getByLabelText("Label"), { key: "Enter" });
  await waitFor(() => screen.getByText("gate"));
  expect(liveLabel).toHaveBeenCalledWith(0xe573f564, "gate");
  fireEvent.click(screen.getByRole("button", { name: /^#e573f564/ }));
  fireEvent.change(screen.getByLabelText("Label"), { target: { value: "" } });
  fireEvent.keyDown(screen.getByLabelText("Label"), { key: "Enter" });
  await waitFor(() => expect(screen.queryByText("gate")).toBeNull());
  expect(liveLabel).toHaveBeenLastCalledWith(0xe573f564, "");
});

test("a failed label keeps the input open with the error", async () => {
  liveLabel.mockResolvedValueOnce({
    status: "error",
    error: "disk full",
  } as never);
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  fireEvent.click(screen.getByRole("button", { name: /^#e573f564/ }));
  fireEvent.keyDown(screen.getByLabelText("Label"), { key: "Enter" });
  await waitFor(() => screen.getByText("disk full"));
  expect(screen.getByLabelText("Label")).toBeTruthy();
});

test("sessions start and end as notes, and earlier rows stay", async () => {
  await recording();
  backend.send({ type: "Ended", reason: "the game closed" });
  backend.send({ type: "Status", status: { state: "Waiting", reason: null } });
  expect(screen.getByText(/Session started 22:51/)).toBeTruthy();
  expect(screen.getByText(/Session ended: the game closed/)).toBeTruthy();
  expect(screen.getByText("Waiting for the game")).toBeTruthy();
  expect(screen.getByText("PlayerStatus.CurrentRupee")).toBeTruthy();
});

test("a muted value shows in the muted count", async () => {
  await recording();
  backend.send({ type: "Muted", hash: 7, name: "#00000007", manual: false });
  expect(screen.getByText("1 muted")).toBeTruthy();
});

test("a reload picks up the session with nothing doubled", async () => {
  const row = send(5, "Mid.Session");
  backend.snapshot = {
    status: { state: "Recording", session },
    events: [{ seq: 0, event: { type: "Started", session } }, row],
    dropped: 0,
    next: backend.next,
  };
  await loaded();
  send(6, "After.Reload");
  expect(screen.getAllByText("Mid.Session")).toHaveLength(1);
  expect(screen.getByText("After.Reload")).toBeTruthy();
  expect(screen.getByText("Recording")).toBeTruthy();
});

test("columns resize by drag and keys, reset on double-click, and persist", async () => {
  await loaded();
  const name = screen.getByRole("separator", { name: "Resize name" });
  const width = () => Number(name.getAttribute("aria-valuenow"));
  fireEvent.pointerDown(name, { clientX: 100 });
  fireEvent.pointerMove(window, { clientX: 150 });
  fireEvent.pointerUp(window);
  fireEvent.pointerMove(window, { clientX: 400 });
  expect(width()).toBe(410);
  fireEvent.keyDown(name, { key: "ArrowLeft" });
  expect(width()).toBe(394);
  cleanup();
  await loaded();
  const again = screen.getByRole("separator", { name: "Resize name" });
  expect(again.getAttribute("aria-valuenow")).toBe("394");
  fireEvent.doubleClick(again);
  expect(again.getAttribute("aria-valuenow")).toBe("360");
});

test("a change draws as old, an arrow, then new", async () => {
  await recording();
  expect(screen.queryByText("1 -> 2")).toBeNull();
  expect(screen.getByText("1")).toBeTruthy();
  expect(screen.getByText("2")).toBeTruthy();
  expect(screen.getByLabelText("to")).toBeTruthy();
});

test("Quiet and the filter are still set after leaving the tab", async () => {
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  fireEvent.change(screen.getByLabelText("Filter lines"), {
    target: { value: "e573" },
  });
  cleanup();
  render(<LiveView />);
  expect(screen.getByLabelText("Filter lines")).toHaveProperty("value", "e573");
  expect(screen.getByText("#e573f564")).toBeTruthy();
});

test("rows new since you last looked flash, and every row in a new session", async () => {
  const flashes = (name: string) =>
    screen.getByText(name).closest(".animate-flash") !== null;
  await recording();
  expect(flashes("PlayerStatus.CurrentRupee")).toBe(true);
  cleanup();
  send(2, "After.Leaving");
  render(<LiveView />);
  expect(flashes("PlayerStatus.CurrentRupee")).toBe(false);
  expect(flashes("After.Leaving")).toBe(true);
  cleanup();
  backend.send({ type: "Started", session: { ...session, log: "/m.log" } });
  render(<LiveView />);
  expect(flashes("PlayerStatus.CurrentRupee")).toBe(true);
});

test("a snapshot that fails says why", async () => {
  backend.fail = "no backend";
  await loaded();
  expect(screen.getByText("no backend")).toBeTruthy();
});

test("says why the log couldn't be shown", async () => {
  await recording();
  backend.revealFail = "No such file";
  fireEvent.click(screen.getByRole("button", { name: "Open log" }));
  expect(await screen.findByText("No such file")).toBeTruthy();
});

test("a row's Copy and Copy shown put clean lines on the clipboard", async () => {
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "Copy row" }));
  await waitFor(() =>
    expect(writeText).toHaveBeenCalledWith(
      "01:15\tPlayerStatus.CurrentRupee\tInt\t1 -> 2",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Copy shown" }));
  await waitFor(() =>
    expect(writeText).toHaveBeenLastCalledWith(
      "--- Session started 22:51 ---\n01:15\tPlayerStatus.CurrentRupee\tInt\t1 -> 2",
    ),
  );
});

test("rows select like a list and copy as lines", async () => {
  await recording();
  send(2, "PlayerStatus.CurrentHeart");
  const line = (text: string) =>
    screen.getByText(text).closest("[data-line]") as HTMLElement;
  const rupee = line("PlayerStatus.CurrentRupee");
  const heart = line("PlayerStatus.CurrentHeart");
  fireEvent.pointerDown(rupee, { button: 0 });
  fireEvent.pointerUp(window);
  fireEvent.pointerDown(heart, { button: 0, shiftKey: true });
  expect(rupee.hasAttribute("data-selected")).toBe(true);
  expect(heart.hasAttribute("data-selected")).toBe(true);
  expect(screen.getByRole("button", { name: "Copy 2 rows" })).toBeTruthy();
  fireEvent.keyDown(document.body, { key: "c", metaKey: true });
  await waitFor(() =>
    expect(writeText).toHaveBeenCalledWith(
      "01:15\tPlayerStatus.CurrentRupee\tInt\t1 -> 2\n01:15\tPlayerStatus.CurrentHeart\tInt\t1 -> 2",
    ),
  );
  fireEvent.pointerDown(heart, { button: 0, metaKey: true });
  expect(heart.hasAttribute("data-selected")).toBe(false);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(rupee.hasAttribute("data-selected")).toBe(false);
  expect(screen.getByRole("button", { name: "Copy shown" })).toBeTruthy();
  fireEvent.pointerDown(
    screen.getAllByRole("button", { name: "Copy row" })[0] as HTMLElement,
    { button: 0 },
  );
  expect(rupee.hasAttribute("data-selected")).toBe(false);
});

test("a drag selects every line it crosses", async () => {
  await recording();
  send(2, "PlayerStatus.CurrentHeart");
  const line = (text: string) =>
    screen.getByText(text).closest("[data-line]") as HTMLElement;
  fireEvent.pointerDown(line("Session started 22:51"), { button: 0 });
  fireEvent.pointerEnter(line("PlayerStatus.CurrentHeart"));
  fireEvent.pointerUp(window);
  fireEvent.pointerEnter(line("Session started 22:51"));
  expect(screen.getByRole("button", { name: "Copy 3 rows" })).toBeTruthy();
});

test("repeats of one value fold into a row until folding is off", async () => {
  await recording();
  send(1, "PlayerStatus.CurrentRupee");
  send(1, "PlayerStatus.CurrentRupee");
  expect(screen.getAllByText("PlayerStatus.CurrentRupee")).toHaveLength(1);
  expect(screen.getByText("×3")).toBeTruthy();
  const toggle = screen.getByRole("button", { name: "Fold repeats" });
  expect(toggle.getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(toggle);
  expect(screen.getAllByText("PlayerStatus.CurrentRupee")).toHaveLength(3);
  expect(screen.queryByText("×3")).toBeNull();
});

test("the Types menu hides a kind of value", async () => {
  await recording();
  backend.send({
    type: "Row",
    at: 80,
    save: false,
    change: {
      hash: 3,
      name: "Scene.Flag",
      label: null,
      kind: "Bool",
      change: "false -> true",
    },
  });
  const types = screen.getByRole("button", { name: "Types" });
  fireEvent.keyDown(types, { key: "Enter" });
  const bool = await screen.findByRole("menuitemcheckbox", { name: "Bool" });
  fireEvent.click(bool);
  fireEvent.keyDown(bool, { key: "Escape" });
  expect(screen.queryByText("Scene.Flag")).toBeNull();
  expect(screen.getByText("PlayerStatus.CurrentRupee")).toBeTruthy();
  expect(screen.getByRole("button", { name: /Types.*1 hidden/ })).toBeTruthy();
});

test("Mark, or M, marks the log with what happened while recording", async () => {
  await loaded();
  expect(
    screen.getByRole("button", { name: "Mark" }).hasAttribute("disabled"),
  ).toBe(true);
  cleanup();
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "Mark" }));
  const what = await screen.findByLabelText("What happened?");
  fireEvent.change(what, { target: { value: "opened the chest" } });
  fireEvent.submit(what);
  await waitFor(() =>
    expect(liveMark).toHaveBeenCalledWith("opened the chest"),
  );
  await waitFor(() =>
    expect(screen.queryByLabelText("What happened?")).toBeNull(),
  );
  fireEvent.keyDown(screen.getByLabelText("Filter lines"), { key: "m" });
  expect(screen.queryByLabelText("What happened?")).toBeNull();
  fireEvent.keyDown(document.body, { key: "m" });
  expect(await screen.findByLabelText("What happened?")).toBeTruthy();
  backend.send({ type: "Marked", at: 75, text: "opened the chest" });
  expect(screen.getByText("01:15 · opened the chest")).toBeTruthy();
});

test("a row's Mute asks the session to mute its value", async () => {
  await recording();
  fireEvent.click(screen.getByRole("button", { name: "Mute this value" }));
  await waitFor(() => expect(liveMute).toHaveBeenCalledWith(1));
  backend.send({
    type: "Muted",
    hash: 1,
    name: "PlayerStatus.CurrentRupee",
    manual: true,
  });
  expect(screen.queryByText("PlayerStatus.CurrentRupee")).toBeNull();
  expect(screen.getByText("1 muted")).toBeTruthy();
});
