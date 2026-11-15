import { RequestError } from "@/components/request-error";
import { Button } from "@/components/ui/button";
import { DialogClose, DialogFooter } from "@/components/ui/dialog";
import { useRequest } from "@/lib/request";

/** A form dialog's foot: why the request around it failed, then Cancel and
 * Save, which waits while the request runs. */
export function SaveFooter() {
  const { busy } = useRequest();
  return (
    <>
      <RequestError />
      <DialogFooter className="pt-1">
        <DialogClose asChild>
          <Button type="button" variant="ghost">
            Cancel
          </Button>
        </DialogClose>
        <Button type="submit" disabled={busy}>
          Save
        </Button>
      </DialogFooter>
    </>
  );
}
