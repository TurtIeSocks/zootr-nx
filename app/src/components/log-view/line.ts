import type { HTMLAttributes } from "react";

/** What a line does with the pointer, for the log's selection. */
export type LinePointer = Pick<
  HTMLAttributes<HTMLElement>,
  "onPointerDown" | "onPointerEnter"
>;

/** The pointer handlers and fill a line carries. Each line is
 * `data-line`, so a press off every line clears the selection. */
export interface LineProps {
  /** Picked in the log's selection. */
  selected: boolean;
  pointer: LinePointer;
}

/** A selected line's fill, over its hover. */
export const PICKED =
  "data-selected:bg-primary-soft data-selected:hover:bg-primary-soft";
