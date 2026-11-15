import type { ReactNode } from "react";
import { InlineError } from "@/components/callout";
import { Label } from "@/components/ui/label";

/** Props for {@link Field}. */
export interface FieldProps {
  /** The control's id, which the label is for. */
  id?: string;
  /** The label's own id, for a control that names itself by it. */
  labelId?: string;
  label: string;
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
  hint,
  warn,
  children,
}: FieldProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label id={labelId} htmlFor={id}>
        {label}
      </Label>
      {children}
      {warn ? (
        <InlineError error={warn} />
      ) : (
        hint && <p className="text-ink-muted text-xs">{hint}</p>
      )}
    </div>
  );
}
