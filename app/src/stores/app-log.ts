import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { commands, events, type Line } from "@/bindings";
import { message } from "@/lib/api";
import { type AppLog, add, EMPTY } from "@/logs/lines";
import { once } from "@/stores/once";

/** The app's own log, which runs in the backend, picked back up from it
 * after a reload. Start it with {@link init}. */
export const useAppLog = create<AppLog>()(dotPath((): AppLog => EMPTY));

const push = (lines: Line[]) => useAppLog.setState((s) => add(s, lines));

/** Listens for new lines and picks up the backlog. */
const started = once(async () => {
  // Listen first, then snapshot: lines from before the snapshot wait, and
  // `add` drops any it already holds.
  let early: Line[] | undefined = [];
  const unlisten = await events.appLog.listen(({ payload }) =>
    early ? early.push(payload) : push([payload]),
  );
  try {
    const { lines, file, unsaved } = await commands.appLogState();
    useAppLog.setState({ file, unsaved });
    push(lines);
  } catch (e) {
    useAppLog.setPath("error", message(e));
  }
  push(early);
  early = undefined;
  return unlisten;
});

export const { init, stop } = started;
