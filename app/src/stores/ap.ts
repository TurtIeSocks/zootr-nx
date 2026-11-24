import { create } from "zustand";
import {
  type ApEvent,
  type ApStatus,
  commands,
  events,
  type Summary,
} from "@/bindings";
import { call, message } from "@/lib/api";
import { once } from "@/stores/once";
import { type Ui, useUi } from "@/stores/ui";

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

/** The most lines kept: as many as the client keeps (`archipelago.rs`). */
export const KEPT = 200;

/** `lines` with `line` after them, the newest {@link KEPT} of them. */
export const appended = (lines: ApLine[], line: ApLine) =>
  [...lines, line].slice(-KEPT);

export const useAp = create<Ap>()(() => ({
  status: "offline",
  lines: [],
  error: null,
}));

/** `now` with the client's `event` in it: its status and line, after the
 * old lines and error when it isn't `clear`ing them for another seed. */
export const applied = (
  now: Pick<Ap, "lines" | "error">,
  event: ApEvent,
): Pick<Ap, "status" | "lines" | "error"> => {
  const before = event.clear ? [] : now.lines;
  return {
    status: event.status,
    lines: event.line
      ? appended(before, { time: Date.now(), text: event.line })
      : before,
    error: event.clear ? null : now.error,
  };
};

/** Listens for the client's notes, then takes what it has said so far.
 * Those earlier notes carry the time they were read. */
const started = once(async () => {
  const unlisten = await events.apEvent.listen(({ payload }) =>
    useAp.setState((s) => applied(s, payload)),
  );
  const { status, log } = await commands.apState();
  const time = Date.now();
  useAp.setState({ status, lines: log.map((text) => ({ time, text })) });
  return unlisten;
});

export const { init, stop } = started;

/** The server and password kept for `entry` in `rooms`. Until any are,
 * the server its `.apznx` names, when it came from the room page. */
export const roomIn = (
  rooms: Ui["ap"]["rooms"],
  entry: Pick<Summary, "id" | "ap_server">,
) => rooms[entry.id] ?? { server: entry.ap_server ?? "", password: "" };

/** Connects the running game to `entry`'s room. */
export async function connect(entry: Pick<Summary, "id" | "ap_server">) {
  const { server, password } = roomIn(useUi.getPath("ap.rooms"), entry);
  useAp.setState({ error: null });
  try {
    await call(commands.apConnect(server, password || null));
  } catch (e) {
    useAp.setState({ error: message(e) });
  }
}

/** Leaves the room, or stops trying to reach it. */
export const disconnect = () => void commands.apDisconnect();
