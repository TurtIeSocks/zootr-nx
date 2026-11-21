/** Class names the dev tabs' lists share. */

/** A scrolling list in a card. */
export const LIST =
  "overflow-auto rounded-lg border border-line bg-surface shadow-xs";

/** A list row: its main button, then actions shown on hover. */
export const ROW =
  "flex items-center gap-1 pr-2 text-[13px] hover:bg-surface-hover [&_svg]:size-3.5 [&_svg]:shrink-0";
/** A row's main button. */
export const ROW_BUTTON =
  "flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-1 pl-3 text-left disabled:cursor-not-allowed disabled:opacity-45 [&>svg]:text-ink-faint";
/** A row's action, shown on hover or focus. */
export const HOVER_ACTION =
  "flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-xs opacity-0 hover:bg-surface-sunken hover:text-ink focus-visible:opacity-100 disabled:cursor-not-allowed group-hover:opacity-100 group-hover:disabled:opacity-45";
