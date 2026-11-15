import { Upload } from "lucide-react";
import { type FormEvent, useState } from "react";
import { commands, type Target } from "@/bindings";
import { InlineError } from "@/components/callout";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { call } from "@/lib/api";
import { useRequestState } from "@/lib/request";

/** Props for {@link ImportTargetDialog}. */
interface ImportTargetDialogProps {
  /** Takes the target read from the pasted text. */
  onImport: (target: Target) => void;
}

/** An Import button that opens a dialog to paste a shared target in. */
export function ImportTargetDialog({ onImport }: ImportTargetDialogProps) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="secondary">
          <Upload /> Import
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Import a target</DialogTitle>
          <DialogDescription>
            Paste a target someone shared. It is added as a new target for you
            to check, then save.
          </DialogDescription>
        </DialogHeader>
        <ImportForm
          onImport={(t) => {
            onImport(t);
            setOpen(false);
          }}
        />
      </DialogContent>
    </Dialog>
  );
}

/** The pasted text and its request. It mounts with the dialog's content, so
 * each opening starts empty. */
function ImportForm({ onImport }: ImportTargetDialogProps) {
  const [text, setText] = useState("");
  const action = useRequestState();
  const submit = (e: FormEvent) => {
    e.preventDefault();
    action.run(async () => onImport(await call(commands.importTarget(text))));
  };

  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
      <Textarea
        autoFocus
        aria-label="Target"
        className="type-mono"
        placeholder='{ "name": "...", "app": "...", "mod_dir": "..." }'
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <InlineError error={action.error} />
      <DialogFooter className="pt-1">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" disabled={action.busy || !text.trim()}>
          Import
        </Button>
      </DialogFooter>
    </form>
  );
}
