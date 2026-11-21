import { useEffect } from "react";
import { ArmedButton } from "@/components/armed-button";
import { InlineError } from "@/components/callout";
import { FloatingBar } from "@/components/floating-bar";
import { WhyButton } from "@/components/why-button";
import { claimFoot } from "@/stores/shell";

/** Props for {@link UnsavedBar}. */
interface UnsavedBarProps {
  /** What's unsaved, while nothing has failed. */
  message: string;
  /** Why the last save failed, in the message's place. */
  error?: string;
  /** Holds both buttons while a save runs. */
  busy: boolean;
  /** The Save button's text, naming what it saves. */
  saveLabel: string;
  /** Why Save can't act now, if it can't. */
  why?: string;
  onDiscard: () => void;
  onSave: () => void;
}

/** A page's unsaved edits: what they are or why saving them failed, then
 * Discard and Save. Render it only while there's something to save. */
export function UnsavedBar({
  message,
  error,
  busy,
  saveLabel,
  why,
  onDiscard,
  onSave,
}: UnsavedBarProps) {
  useEffect(claimFoot, []);
  return (
    <FloatingBar
      message={error ? <InlineError error={error} /> : <p>{message}</p>}
    >
      <ArmedButton verb="discard" disabled={busy} onAct={onDiscard}>
        Discard
      </ArmedButton>
      <WhyButton disabled={busy} why={why} onClick={onSave}>
        {saveLabel}
      </WhyButton>
    </FloatingBar>
  );
}
