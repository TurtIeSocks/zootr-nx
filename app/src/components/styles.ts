/** Class strings shared by several components, kept apart from them so
 * component modules export only components. */

/** For a card's list of rows: inset rules between rows that vanish around
 * the row under the pointer. Rows need `relative`. */
export const divided =
  "[&>*+*]:before:absolute [&>*+*]:before:inset-x-3 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-line [&>*+*]:before:transition-opacity [&>*:hover+*]:before:opacity-0 [&>*+*:hover]:before:opacity-0";

/** The outline every text field shares: `control-border`, darker on hover,
 * `focus` with a 1px ring while typing, `danger` when invalid. */
export const fieldClass =
  "w-full min-w-0 rounded-md border border-control-border bg-surface text-[13px] text-ink outline-none transition-[border-color,box-shadow] duration-(--duration-fast) ease-out placeholder:text-ink-faint hover:border-ink-muted focus-visible:border-focus focus-visible:shadow-[0_0_0_1px_var(--focus)] focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-45 aria-invalid:border-danger aria-invalid:shadow-[0_0_0_1px_var(--danger)]";

/** A button that reads as a link, inside running text. */
export const linkClass =
  "cursor-pointer font-medium text-primary-ink underline-offset-3 hover:underline";

/** What every cell in a check's or a door's row shares. */
export const cellClass = "h-8.5 border-line border-t px-3 align-middle";
