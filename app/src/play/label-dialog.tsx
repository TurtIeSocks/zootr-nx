import { type FormEvent, useState } from "react";
import type { Summary } from "@/bindings";
import { SaveFooter } from "@/components/save-footer";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { label } from "@/stores/seeds";
import { closeLabel, useShell } from "@/stores/shell";

/** Names a history seed and notes what it's for. */
export function LabelDialog() {
  const [entry] = useShell.usePath("labeling");
  return (
    <Dialog
      open={entry !== undefined}
      onOpenChange={(open) => !open && closeLabel()}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Label Seed</DialogTitle>
          <DialogDescription>
            Seed <span className="font-mono text-ink">{entry?.seed}</span>
          </DialogDescription>
        </DialogHeader>
        {/* Another seed starts a fresh form and request. */}
        <RequestProvider key={entry?.id}>
          <LabelForm entry={entry} />
        </RequestProvider>
      </DialogContent>
    </Dialog>
  );
}

/** Props for {@link LabelForm}. */
interface LabelFormProps {
  /** The seed being labelled; none while the dialog closes. */
  entry: Summary | undefined;
}

/** The label dialog's form, saving through the request around it. */
function LabelForm({ entry }: LabelFormProps) {
  const [name, setName] = useState(entry?.label.name ?? "");
  const [note, setNote] = useState(entry?.label.note ?? "");
  const save = useRequest();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!entry) return;
    save.run(async () => {
      await label(entry.id, { name: name.trim(), note: note.trim() });
      closeLabel();
    });
  };

  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
      <Input
        autoFocus
        aria-label="Name"
        placeholder="Name"
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <Textarea
        aria-label="Note"
        placeholder="Note"
        value={note}
        onChange={(e) => setNote(e.target.value)}
      />
      <SaveFooter />
    </form>
  );
}
