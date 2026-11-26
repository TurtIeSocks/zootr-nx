import { SlidersHorizontal } from "lucide-react";
import type { ReactNode } from "react";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { useArmed } from "@/play/use-armed";

/** Props for {@link ListRow}. */
export interface ListRowProps {
  /** Its id, for search to reveal it. */
  id?: string;
  /** Before the text, such as a seed's icons. */
  lead?: ReactNode;
  /** The row's name, over its quiet line. */
  name: ReactNode;
  /** The quiet line under the name. */
  meta: ReactNode;
  /** Stays visible at the row's end, such as Installed. */
  mark?: ReactNode;
  /** Shown over the row's end on hover or focus, so they take no room. */
  actions: ReactNode;
  /** Makes it pickable: a checkbox takes the lead's place, and a click
   * anywhere on the row picks it, `range` with shift held. */
  select?: { label: string; on: boolean; toggle: (range: boolean) => void };
}

/** A row of History or Favorites: a name over a quiet line, with its
 * actions a hover away. Off screen, it skips layout. */
export function ListRow({
  id,
  lead,
  name,
  meta,
  mark,
  actions,
  select,
}: ListRowProps) {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the checkbox inside is the keyboard's way to pick it
    <li
      id={id}
      data-reveal={id ? true : undefined}
      onClick={select && ((e) => select.toggle(e.shiftKey))}
      className={cn(
        "group/row relative flex min-h-14 items-center gap-3 rounded-md px-3 py-2 transition-colors duration-(--duration-fast) [contain-intrinsic-size:auto_3.5rem] [content-visibility:auto] focus-within:bg-surface-hover hover:bg-surface-hover",
        select && "cursor-pointer select-none",
      )}
    >
      {select ? (
        <Checkbox
          aria-label={select.label}
          checked={select.on}
          // The row's click picks it, keys included: the box only shows it.
          onClick={(e) => e.preventDefault()}
        />
      ) : (
        lead
      )}
      <div className="min-w-0 flex-1">
        {name}
        <div className="mt-0.5 flex items-center gap-1.5 overflow-hidden whitespace-nowrap text-ink-faint text-xs">
          {meta}
        </div>
      </div>
      {mark}
      <div className="absolute inset-y-0 right-3 flex items-center gap-0.5 pointer-events-none bg-surface-hover pl-2 opacity-0 transition-opacity duration-(--duration-fast) group-focus-within/row:pointer-events-auto group-focus-within/row:opacity-100 group-hover/row:pointer-events-auto group-hover/row:opacity-100">
        {actions}
      </div>
    </li>
  );
}

/** "differs in N settings", or nothing at none. */
export function DiffersIn({ n }: { n: number }) {
  return (
    n > 0 && (
      <span>
        · differs in {n} setting{n === 1 ? "" : "s"}
      </span>
    )
  );
}

/** Props for {@link UseSettings}. */
export interface UseSettingsProps {
  /** How many settings differ from the current ones. */
  differs: number;
  /** What using them does, said on hover. */
  tip: string;
  onUse: () => void;
}

/** Makes a row's settings the current ones, unless they already are. */
export function UseSettings({ differs, tip, onUse }: UseSettingsProps) {
  return (
    <TipButton
      label="Use settings"
      tip={differs === 0 ? "Your settings already match" : tip}
      variant="ghost"
      disabled={differs === 0}
      onClick={onUse}
    >
      <SlidersHorizontal />
    </TipButton>
  );
}

/** Props for {@link ConfirmText}. */
export interface ConfirmTextProps {
  /** What it says until clicked. */
  label: string;
  tip: string;
  /** What it says once armed, waiting for the second click. */
  confirm: string;
  disabled?: boolean;
  onConfirm: () => void;
}

/** A text button that acts on a second click, saying `confirm` between. */
export function ConfirmText({
  label,
  tip,
  confirm,
  disabled,
  onConfirm,
}: ConfirmTextProps) {
  const { armed, ...handlers } = useArmed(onConfirm);
  return (
    // A disabled button gets no pointer events: the wrap keeps its tip.
    <Tip tip={tip} wrap={disabled}>
      <Button
        variant={armed ? "destructive" : "ghost"}
        size="sm"
        disabled={disabled}
        {...handlers}
      >
        {armed ? confirm : label}
      </Button>
    </Tip>
  );
}

/** Props for {@link ConfirmButton}. */
export interface ConfirmButtonProps {
  label: string;
  tip: string;
  /** What it says once armed, waiting for the second click. */
  confirm: string;
  icon: ReactNode;
  disabled?: boolean;
  onConfirm: () => void;
}

/** An icon button that acts on a second click, saying `confirm` between. */
export function ConfirmButton({
  label,
  tip,
  confirm,
  icon,
  disabled,
  onConfirm,
}: ConfirmButtonProps) {
  const { armed, ...handlers } = useArmed(onConfirm);
  return (
    <TipButton
      label={label}
      tip={armed ? confirm : tip}
      variant={armed ? "destructive" : "ghost"}
      size={armed ? "sm" : "icon-sm"}
      disabled={disabled}
      {...handlers}
    >
      {armed ? confirm : icon}
    </TipButton>
  );
}
