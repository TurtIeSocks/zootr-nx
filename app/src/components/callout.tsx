import { CircleAlert, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
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
    <p
      role="alert"
      className={cn(
        "flex items-center gap-1.5 text-[12.5px] text-danger-ink",
        className,
      )}
    >
      <CircleAlert className="size-3.5 shrink-0" />
      {error}
    </p>
  );
}
