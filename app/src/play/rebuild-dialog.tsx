import type { Build } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

/** Props for {@link RebuildDialog}. */
interface RebuildDialogProps {
  /** The target about to start; the dialog is open while it's set. */
  name: string | undefined;
  build: Build | null | undefined;
  onClose: () => void;
  onRebuild: () => void;
  onPlay: () => void;
}

/** Asked before Play starts a game whose build has another hook patch
 * than the installed one: Rebuild, Play anyway, or Cancel. */
export function RebuildDialog({
  name,
  build,
  onClose,
  onRebuild,
  onPlay,
}: RebuildDialogProps) {
  const then = (f: () => void) => () => {
    onClose();
    f();
  };
  return (
    <Dialog open={name !== undefined} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rebuild {name} first?</DialogTitle>
          <DialogDescription>
            {build?.text}.{" "}
            {build?.command
              ? "Until it's rebuilt, the game runs an older hook."
              : "Until it's rebuilt by hand, the game runs an older hook."}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="secondary" onClick={then(onPlay)}>
            Play anyway
          </Button>
          {build?.command && <Button onClick={then(onRebuild)}>Rebuild</Button>}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
