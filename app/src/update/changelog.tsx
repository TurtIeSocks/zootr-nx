import { Loader2, ScrollText } from "lucide-react";
import { useEffect, useState } from "react";
import { InlineError } from "@/components/callout";
import { TipButton } from "@/components/tip-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { askedRepo, useUpdate } from "@/stores/update";
import { Notes, ReleaseLink } from "@/update/notes";
import { day, fetchReleases, PAGE, type Release } from "@/update/release";

/** Opens every release's notes, newest first. Nothing is fetched until it
 * opens. */
export function ChangelogButton() {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <TipButton
        label="Changelog"
        tip="Changelog"
        variant="ghost"
        onClick={() => setOpen(true)}
      >
        <ScrollText />
      </TipButton>
      <DialogContent className="flex max-h-[80vh] max-w-[min(640px,calc(100%-4rem))] flex-col">
        <DialogHeader>
          <DialogTitle>Changelog</DialogTitle>
          <DialogDescription>What each release changed.</DialogDescription>
        </DialogHeader>
        {/* The content unmounts while closed, so each open fetches anew. */}
        <Releases />
      </DialogContent>
    </Dialog>
  );
}

/** Where the list stands: fetching, more to fetch, all fetched, or failed. */
type Status = "loading" | "more" | "end" | "error";

/** The releases, a page at a time: the first on mount, each older page
 * when asked for. */
function Releases() {
  const [pages, setPages] = useState(1);
  const [list, setList] = useState<Release[]>([]);
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<string>();
  useEffect(() => {
    let live = true;
    setStatus("loading");
    fetchReleases(askedRepo(), pages).then(
      (got) => {
        if (!live) return;
        setList((l) => [...l, ...got]);
        setStatus(got.length < PAGE ? "end" : "more");
      },
      (e: unknown) => {
        if (!live) return;
        setError(String(e));
        setStatus("error");
      },
    );
    return () => {
      live = false;
    };
  }, [pages]);
  return (
    <div className="-mx-5 min-h-0 flex-1 overflow-auto border-line border-t px-5">
      {list.map((r) => (
        <ReleaseNotes key={r.version} release={r} />
      ))}
      <div className="flex justify-center py-4">
        {status === "loading" && (
          <Loader2 className="size-4 animate-spin text-ink-muted" />
        )}
        {status === "more" && (
          <Button variant="ghost" size="sm" onClick={() => setPages(pages + 1)}>
            Load older releases
          </Button>
        )}
        {status === "end" && list.length === 0 && (
          <p className="text-ink-muted text-sm">No releases yet</p>
        )}
        {status === "error" && (
          <InlineError
            error={`Couldn't reach GitHub. Try again later\n\n${error}`}
          />
        )}
      </div>
    </div>
  );
}

/** Props for {@link ReleaseNotes}. */
interface ReleaseNotesProps {
  release: Release;
}

/** One release: its version, date and page, then its notes. */
function ReleaseNotes({ release }: ReleaseNotesProps) {
  const [current] = useUpdate.usePath("current");
  return (
    <article className="border-line border-b py-4 last-of-type:border-b-0">
      <div className="flex items-center gap-2">
        <h3 className="font-medium text-sm">v{release.version}</h3>
        {release.version === current && <Badge variant="info">Installed</Badge>}
        <span className="ml-auto flex items-center gap-3 text-ink-faint text-xs">
          {release.published && day(release.published)}
          <ReleaseLink href={release.url}>Release page</ReleaseLink>
        </span>
      </div>
      <Notes md={release.notes} className="pt-1.5" />
    </article>
  );
}
