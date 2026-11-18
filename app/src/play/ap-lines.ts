import type { ApLine } from "@/stores/ap";

/** What a client note is about, read from how it starts. */
export type LineKind =
  | "sent"
  | "received"
  | "death"
  | "goal"
  | "lost"
  | "refused"
  | "room"
  | "trouble";

/** Each note's opening words, as zootr's AP client writes them. */
const STARTS: [string, LineKind][] = [
  ["sent ", "sent"],
  ["received ", "received"],
  ["DeathLink:", "death"],
  ["Link died", "death"],
  ["goal sent", "goal"],
  ["lost ", "lost"],
  ["refused:", "refused"],
  ["connecting", "room"],
  ["connected", "room"],
];

/** A note's kind. Anything else the client says is trouble. */
export const kindOf = (text: string): LineKind =>
  STARTS.find(([start]) => text.startsWith(start))?.[1] ?? "trouble";

/** The log's filters: everything, items sent and received, or the rest. */
export type LogFilter = "all" | "items" | "room";

/** Whether `filter` keeps a note of `kind`. */
export function keeps(filter: LogFilter, kind: LineKind) {
  const item = kind === "sent" || kind === "received";
  return filter === "all" || (filter === "items") === item;
}

/** The client lost its room and is trying again. */
export const reconnecting = (lines: ApLine[]) =>
  kindOf(lines.at(-1)?.text ?? "") === "lost";

/** The server's refusals, as `archipelago_rs` words them, in a sentence. */
const REFUSALS: Record<string, (slot: string) => string> = {
  "the name provided doesn't match any names on the server": (slot) =>
    `The room has no slot named ${slot}.`,
  "invalid or missing password": () => "The password is wrong or missing.",
  "this player isn't playing the expected game": (slot) =>
    `${slot} plays another game in this room.`,
};

/** Capitalized and stopped. */
const sentence = (text: string) =>
  text.charAt(0).toUpperCase() +
  text.slice(1) +
  (/[.!?]$/.test(text) ? "" : ".");

/** Why the room refused `slot`, from the client's last refusal. */
export function refusal(lines: ApLine[], slot: string) {
  const last = [...lines].reverse().find((l) => kindOf(l.text) === "refused");
  const why = (last?.text ?? "")
    .replace(/^refused: /, "")
    .replace(/^Archipelago refused connection: /, "");
  if (!why) return "The room refused the connection.";
  return why
    .split(", ")
    .map((r) => REFUSALS[r]?.(slot) ?? sentence(r))
    .join(" ");
}
