import { create } from "zustand";
import { type ApStatus, commands, events } from "@/bindings";
import { call, message } from "@/lib/api";
import { once } from "@/stores/once";
import { useUi } from "@/stores/ui";

/** One of the client's notes, with when the app heard it. */
export interface ApLine {
  /** By `Date.now()`. */
  time: number;
  text: string;
}

/** The Archipelago client's status and log, kept current by its events. */
interface Ap {
  status: ApStatus;
  lines: ApLine[];
  /** Why the last connect couldn't start, before the client took it. */
  error: string | null;
}

export const useAp = create<Ap>()(() => ({
  status: "offline",
  lines: [],
  error: null,
}));

/** Listens for the client's notes, then takes what it has said so far.
 * Those earlier notes carry the time they were read. */
const started = once(async () => {
  const unlisten = await events.apEvent.listen(({ payload }) =>
    useAp.setState((s) => ({
      status: payload.status,
      lines: payload.line
        ? [...s.lines, { time: Date.now(), text: payload.line }]
        : s.lines,
    })),
  );
  const { status, log } = await commands.apState();
  const time = Date.now();
  useAp.setState({ status, lines: log.map((text) => ({ time, text })) });
  return unlisten;
});

export const { init, stop } = started;

/** The server and password kept for a seed, blank when none are. */
export const roomOf = (id: number) =>
  useUi.getPath("ap.rooms")[id] ?? { server: "", password: "" };

/** Connects the running game to the room kept for seed `id`. */
export async function connect(id: number) {
  const { server, password } = roomOf(id);
  useAp.setState({ error: null });
  try {
    await call(commands.apConnect(server, password || null));
  } catch (e) {
    useAp.setState({ error: message(e) });
  }
}

/** Leaves the room, or stops trying to reach it. */
export const disconnect = () => void commands.apDisconnect();
