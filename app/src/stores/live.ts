import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { commands, events, type Numbered } from "@/bindings";
import { call, message } from "@/lib/api";
import { type Action, IDLE, type Live, reduce } from "@/live/rows";
import { relabel } from "@/stores/checks";
import { once } from "@/stores/once";

/** The Live tab's sessions, which run in the backend, picked back up from
 * it after a reload. */
export interface LiveState extends Live {
  /** Why picking the session back up failed. */
  error?: string;
}

/** The live store. Start it with {@link init}. */
export const useLive = create<LiveState>()(dotPath((): LiveState => IDLE));

// zustand-dot has no path for the root, so the reducer replaces it whole.
const dispatch = (a: Action) => useLive.setState((s) => reduce(s, a), true);

/** Listens for the watcher's events and picks up its snapshot. */
const started = once(async () => {
  // Listen first, then snapshot: events from before the snapshot wait,
  // and the reducer drops any it already holds.
  let early: Numbered[] | undefined = [];
  const event = (n: Numbered) => dispatch({ type: "Event", ...n });
  const unlisten = await events.liveEvent.listen(({ payload }) =>
    early ? early.push(payload) : event(payload),
  );
  try {
    dispatch({ type: "Snapshot", snapshot: await commands.liveState() });
  } catch (e) {
    useLive.setPath("error", message(e));
  }
  early.forEach(event);
  early = undefined;
  return unlisten;
});

export const { init, stop } = started;

/** Labels a flag, in Live and in Checks alike. Blank drops it. */
export async function label(hash: number, text: string) {
  await call(commands.liveLabel(hash, text));
  dispatch({ type: "Labelled", hash, label: text.trim() });
  relabel(hash, text.trim());
}

/** Marks the session's log with what happened. It comes back as an event. */
export const mark = (what: string) => call(commands.liveMark(what));

/** Leaves a value out of the session from now on, and off the screen. */
export const mute = (hash: number) => call(commands.liveMute(hash));

/** Empties the log on screen. The session log keeps everything. */
export const clear = () => dispatch({ type: "Cleared" });
