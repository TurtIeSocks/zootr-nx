import type { ReactNode } from "react";
import { Help } from "@/components/help";
import { Tip } from "@/components/tip";
import { Badge, ChangedDot } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import type { HelpTopic } from "@/help/topics";
import { cn } from "@/lib/utils";
import { untested } from "@/settings/setting";

/** Marks a setting not yet played through. */
export function UntestedBadge() {
  return (
    <Tip focusable tip="Not played through yet, so it may misbehave">
      <Badge variant="gold">Untested</Badge>
    </Tip>
  );
}

/** Props for {@link Row}. */
export interface RowProps {
  /** The control's id. The label is `${id}-label`. */
  id: string;
  label: string;
  /** A help bubble beside the label. */
  help?: HelpTopic;
  /** Its "Untested." prefix becomes a badge. */
  description: string;
  /** Marks the row with the gold dot. */
  changed?: boolean;
  /** Mutes the label. */
  locked?: boolean;
  /** A line under the description. */
  under?: ReactNode;
  children: ReactNode;
}

/** A row's frame: label with any badge, the full description, and the
 * control at the trailing edge. */
export function Row({
  id,
  label,
  help,
  description,
  changed,
  locked,
  under,
  children,
}: RowProps) {
  const [isUntested, text] = untested(description);
  return (
    <div className="group/row relative grid min-h-15 grid-cols-[minmax(0,1fr)_auto] items-center gap-6 rounded-md p-3 transition-colors duration-(--duration-fast) ease-out hover:bg-surface-hover">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          {changed && (
            <span role="img" aria-label="changed" className="-ml-3.5 flex">
              <ChangedDot />
            </span>
          )}
          <Label
            id={`${id}-label`}
            htmlFor={id}
            className={cn(
              "text-sm leading-5",
              locked ? "text-ink-muted" : "cursor-pointer",
            )}
          >
            {label}
          </Label>
          {help && <Help topic={help} />}
          {isUntested && <UntestedBadge />}
        </div>
        {text && (
          <p className="mt-0.5 max-w-[62ch] text-pretty text-[13px] text-ink-muted leading-[18px]">
            {text}
          </p>
        )}
        {under}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}
