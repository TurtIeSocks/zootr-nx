import type { Summary } from "@/bindings";

/** Past this many things, a summary gives a count instead. */
export const LISTED = 4;

/** `n` things, as "1 item" or "3 items". */
export const count = (n: number, one: string) =>
  `${n} ${one}${n === 1 ? "" : "s"}`;

/** Starts every share code, as `share.rs` writes it: `ZR2-`, or `ZR1-`
 * from before the defaults went vanilla. */
const SHARE_PREFIX = /^ZR[12]-/;

/** Whether `text` is a share code rather than a seed number. */
export const isShareCode = (text: string) => SHARE_PREFIX.test(text.trim());

/** A pasted share code as is, anything else as the digits in it. */
export function seedOrCode(text: string) {
  return isShareCode(text) ? text.trim() : text.replace(/\D/g, "");
}

/** What a seed is called: its label, else an Archipelago slot's name.
 * Blank for an unnamed standard seed. */
export const nameOf = (entry: Pick<Summary, "label" | "ap_slot">) =>
  entry.label.name || entry.ap_slot || "";

/** The non-blank lines of `text`, trimmed. */
export const splitLines = (text: string) =>
  text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

/** A time in seconds since the epoch, as a local date and time. */
export function when(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** How long before `now` a time in milliseconds was: "just now", "3m ago",
 * "2h ago", "4d ago". */
export function ago(ms: number, now = Date.now()): string {
  const minutes = Math.floor((now - ms) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours}h ago` : `${Math.floor(hours / 24)}d ago`;
}
