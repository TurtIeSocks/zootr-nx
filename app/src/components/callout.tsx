import { CircleAlert, type LucideIcon } from "lucide-react";
import { type ReactNode, useState } from "react";
import { errorParts } from "@/lib/api";
import { cn } from "@/lib/utils";

const TONES = {
  neutral: "bg-surface-sunken [&>svg]:text-ink-muted",
  info: "bg-info-soft [&>svg]:text-info-ink",
  gold: "bg-gold-soft [&>svg]:text-gold-ink",
  danger: "bg-danger-soft [&>svg]:text-danger-ink",
  green: "bg-primary-soft [&>svg]:text-primary-ink",
};

/** Props for {@link Callout}. */
interface CalloutProps {
  tone?: keyof typeof TONES;
  icon: LucideIcon;
  /** A link-styled action at the trailing edge. */
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}

/** An inline message about the thing just above it. */
export function Callout({
  tone = "neutral",
  icon: Icon,
  action,
  className,
  children,
}: CalloutProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-md px-3 py-2 text-[13px] text-ink leading-[18px] [&>svg]:mt-px [&>svg]:size-4 [&>svg]:shrink-0",
        TONES[tone],
        className,
      )}
    >
      <Icon />
      <div className="min-w-0 flex-1">{children}</div>
      {action && (
        <div className="shrink-0 self-center whitespace-nowrap pl-2">
          {action}
        </div>
      )}
    </div>
  );
}

/** Props for {@link ErrorWords}. */
interface ErrorWordsProps {
  /** What a player reads, then a blank line and the details, if any. */
  text: string;
  /** Before the words on their line, like an icon. */
  lead?: ReactNode;
}

/** A message's words, with its details behind a toggle when it has any. */
export function ErrorWords({ text, lead }: ErrorWordsProps) {
  const [open, setOpen] = useState(false);
  const { said, details } = errorParts(text);
  return (
    <>
      <p className="flex items-center gap-1.5">
        {lead}
        <span className="min-w-0">{said}</span>
        {details && (
          <button
            type="button"
            aria-expanded={open}
            className="ml-1 shrink-0 cursor-pointer text-ink-muted underline underline-offset-2"
            onClick={() => setOpen(!open)}
          >
            Details
          </button>
        )}
      </p>
      {details && open && (
        <p className="mt-1 select-text whitespace-pre-wrap break-words font-mono text-[11.5px] text-ink-muted">
          {details}
        </p>
      )}
    </>
  );
}

/** Props for {@link InlineError}. */
interface InlineErrorProps {
  error?: string;
  className?: string;
}

/** What failed, under the control that failed. Nothing when there's no
 * error. */
export function InlineError({ error, className }: InlineErrorProps) {
  if (!error) return null;
  return (
    <div
      role="alert"
      className={cn("text-[12.5px] text-danger-ink", className)}
    >
      <ErrorWords
        text={error}
        lead={<CircleAlert className="size-3.5 shrink-0" />}
      />
    </div>
  );
}
