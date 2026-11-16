import { type ReactNode, useEffect, useState } from "react";
import type { Info } from "@/bindings";
import { InlineError } from "@/components/callout";
import { ValueInput } from "@/components/value-input";
import { message } from "@/lib/api";

/** Props for {@link StagedInput}. */
interface StagedInputProps {
  info: Info;
  /** As the save holds it, with what's pending applied. */
  value: string;
  label: string;
  /** Stages `value`. A throw shows under the field and keeps the draft. */
  onCommit: (value: string) => Promise<void>;
  className?: string;
  /** Shown under the field when nothing has failed. */
  note?: ReactNode;
  /** What each choice reads as. */
  labels?: Record<string, string>;
}

/** A value's field that stages what's typed once it's done: a switch or a
 * pick at once, typing on Enter or on leaving the field. */
export function StagedInput({
  info,
  value,
  label,
  onCommit,
  className,
  note,
  labels,
}: StagedInputProps) {
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState<string>();
  // A fresh view replaces the draft.
  useEffect(() => setDraft(value), [value]);
  const commit = (v: string) => {
    if (v === value) return;
    setError(undefined);
    onCommit(v).catch((e) => setError(message(e)));
  };
  const atOnce = info.kind === "Bool" || info.input === "choice";
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: Enter and leaving belong to the fields inside
    <div
      className="flex flex-col gap-1"
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
          e.preventDefault();
          commit(draft);
        }
      }}
      onBlur={(e) => {
        // Only when focus leaves the whole field: a vector has several.
        if (!e.currentTarget.contains(e.relatedTarget)) commit(draft);
      }}
    >
      <ValueInput
        info={info}
        value={draft}
        label={label}
        className={className}
        labels={labels}
        onChange={(v) => {
          setDraft(v);
          if (atOnce) commit(v);
        }}
      />
      {error ? <InlineError error={error} /> : note}
    </div>
  );
}
