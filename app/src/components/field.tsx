import type { ReactNode } from "react";
import { InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { Label } from "@/components/ui/label";
import type { HelpTopic } from "@/help/topics";

/** Props for {@link Field}. */
export interface FieldProps {
  /** The control's id, which the label is for. */
  id?: string;
  /** The label's own id, for a control that names itself by it. */
  labelId?: string;
  label: string;
  /** A help bubble beside the label. */
  help?: HelpTopic;
  /** A line under the control. */
  hint?: string;
  /** Replaces the hint, as an error, while set. */
  warn?: string;
  /** The control. */
  children: ReactNode;
}

/** A labelled control with a hint under it. */
export function Field({
  id,
  labelId,
  label,
  help,
  hint,
  warn,
  children,
}: FieldProps) {
  return (
    <div data-reveal className="flex flex-col gap-1.5">
      <div className="flex items-center gap-1.5">
        <Label id={labelId} htmlFor={id}>
          {label}
        </Label>
        {help && <Help topic={help} />}
      </div>
      {children}
      {warn ? (
        <InlineError error={warn} />
      ) : (
        hint && <p className="text-ink-muted text-xs">{hint}</p>
      )}
    </div>
  );
}
