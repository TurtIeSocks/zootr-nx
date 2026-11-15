import type { Side } from "@/bindings";

/** The sides of an actor a warp lands on, for a picker. */
export const SIDES: { id: Side; label: string }[] = [
  { id: "front", label: "Front" },
  { id: "back", label: "Back" },
  { id: "left", label: "Left" },
  { id: "right", label: "Right" },
];
