import { type FormEvent, type ReactNode, useState } from "react";
import type { Label } from "@/bindings";
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
import { useKept } from "@/play/use-kept";
import { label } from "@/stores/seeds";
import { closeLabel, useShell } from "@/stores/shell";

/** Names a history seed and notes what it's for. */
export function LabelDialog() {
  const [labeling] = useShell.usePath("labeling");
  // The form keeps its seed while the dialog fades out.
  const entry = useKept(labeling);
  return (
    <NameDialog
      open={labeling !== undefined}
      onClose={closeLabel}
      heading="Label Seed"
      description={
        <>
          Seed <span className="font-mono text-ink">{entry?.seed}</span>
        </>
      }
      formKey={entry?.id}
      label={entry?.label}
      onSave={(l) => (entry ? label(entry.id, l) : Promise.resolve())}
    />
  );
}

/** Props for {@link NameDialog}. */
export interface NameDialogProps {
  open: boolean;
  onClose: () => void;
  heading: string;
  description: ReactNode;
  /** What the form starts from. A new key starts a fresh form. */
  formKey: string | number | undefined;
  label: Label | undefined;
  /** Saves the trimmed name and note, throwing why it couldn't. */
  onSave: (label: Label) => Promise<void>;
}

/** A dialog that asks for a name and a note, and closes once they save. */
export function NameDialog({
  open,
  onClose,
  heading,
  description,
  formKey,
  label,
  onSave,
}: NameDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{heading}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <RequestProvider key={formKey}>
          <NameForm label={label} onSave={onSave} onSaved={onClose} />
        </RequestProvider>
      </DialogContent>
    </Dialog>
  );
}

/** Props for {@link NameForm}. */
interface NameFormProps {
  label: Label | undefined;
  onSave: (label: Label) => Promise<void>;
  onSaved: () => void;
}

/** The name dialog's form, saving through the request around it. */
function NameForm({ label, onSave, onSaved }: NameFormProps) {
  const [name, setName] = useState(label?.name ?? "");
  const [note, setNote] = useState(label?.note ?? "");
  const save = useRequest();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.run(async () => {
      await onSave({ name: name.trim(), note: note.trim() });
      onSaved();
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
