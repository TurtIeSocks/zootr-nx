import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AppLogState, Line } from "@/bindings";
import { LogsView } from "@/logs/logs-view";
import { init } from "@/stores/app-log";
import { useConfig } from "@/stores/config";
import { useUi } from "@/stores/ui";

/** A stand-in for the backend: numbers lines and sends them on. */
const backend = {
  listener: undefined as ((e: { payload: Line }) => void) | undefined,
  /** Lines the snapshot holds. */
  held: [] as Line[],
  next: 0,
  line(level: Line["level"], text: string): Line {
    return { seq: backend.next++, time: "12:00:00", level, text };
  },
  send(level: Line["level"], text: string) {
    const l = backend.line(level, text);
    act(() => backend.listener?.({ payload: l }));
  },
};

vi.mock("@/bindings", () => ({
  events: {
    appLog: {
      listen: (cb: (e: { payload: Line }) => void) => {
        backend.listener = cb;
        return Promise.resolve(() => {});
      },
    },
  },
  commands: {
    appLogState: (): Promise<AppLogState> => {
      // A line that lands while the snapshot is on its way arrives twice.
      const late = backend.line("info", "late");
      backend.listener?.({ payload: late });
      return Promise.resolve({
        lines: [...backend.held, late],
        file: "/logs/app-1.log",
        unsaved: null,
      });
    },
  },
}));

const writeText = vi.fn((_text: string) => Promise.resolve());
Object.assign(navigator, { clipboard: { writeText } });

afterEach(() => {
  cleanup();
  writeText.mockClear();
  useUi.setPath("logs", { grep: "", hidden: [] });
});

/** Renders once the page has its snapshot, with the run's first lines. */
async function loaded() {
  backend.held = [
    backend.line("info", "zootr 0.9.0 on macos aarch64"),
    backend.line("error", "Install failed: no dump"),
  ];
  await act(init);
  render(<LogsView />);
}

test("the backlog shows once, then new lines follow", async () => {
  await loaded();
  expect(screen.getByText("zootr 0.9.0 on macos aarch64")).toBeTruthy();
  expect(screen.getAllByText("late")).toHaveLength(1);
  backend.send("warn", "--preset old is now new");
  expect(screen.getByText("--preset old is now new")).toBeTruthy();
});

test("debug lines show only with developer tools, and the menu offers them then", async () => {
  await loaded();
  backend.send("debug", "→ status 0.01 s");
  expect(screen.queryByText("→ status 0.01 s")).toBeNull();
  const menu = screen.getByRole("button", { name: "Levels" });
  fireEvent.keyDown(menu, { key: "Enter" });
  await screen.findByRole("menuitemcheckbox", { name: "error" });
  expect(screen.queryByRole("menuitemcheckbox", { name: "debug" })).toBeNull();
  fireEvent.keyDown(document.activeElement ?? document.body, {
    key: "Escape",
  });
  act(() => useConfig.setPath("config.dev_tools", true));
  expect(screen.getByText("→ status 0.01 s")).toBeTruthy();
  act(() => useConfig.setPath("config.dev_tools", false));
});

test("the filter narrows, and Copy takes what's shown or what's selected", async () => {
  await loaded();
  fireEvent.change(screen.getByLabelText("Filter lines"), {
    target: { value: "install" },
  });
  expect(screen.queryByText("late")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Copy shown" }));
  await waitFor(() =>
    expect(writeText).toHaveBeenLastCalledWith(
      "12:00:00  error  Install failed: no dump",
    ),
  );
  fireEvent.change(screen.getByLabelText("Filter lines"), {
    target: { value: "" },
  });
  fireEvent.pointerDown(screen.getAllByText("late")[0] as Element, {
    button: 0,
  });
  fireEvent.keyDown(document.body, { key: "c", metaKey: true });
  await waitFor(() =>
    expect(writeText).toHaveBeenLastCalledWith("12:00:00  info   late"),
  );
});
