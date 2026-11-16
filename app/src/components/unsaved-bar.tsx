import { InlineError } from "@/components/callout";
import { FloatingBar } from "@/components/floating-bar";
import { Button } from "@/components/ui/button";

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
  onDiscard,
  onSave,
}: UnsavedBarProps) {
  return (
    <FloatingBar
      message={error ? <InlineError error={error} /> : <p>{message}</p>}
    >
      <Button variant="ghost" disabled={busy} onClick={onDiscard}>
        Discard
      </Button>
      <Button disabled={busy} onClick={onSave}>
        {saveLabel}
      </Button>
    </FloatingBar>
  );
}
