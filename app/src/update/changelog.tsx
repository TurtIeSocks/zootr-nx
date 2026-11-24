import { Loader2, Newspaper } from "lucide-react";
import { useEffect, useState } from "react";
import { InlineError } from "@/components/callout";
import { Disclosure } from "@/components/disclosure";
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
import { message } from "@/lib/api";
import { askedRepo, useUpdate } from "@/stores/update";
import { Notes, ReleaseLink } from "@/update/notes";
import { day, fetchReleases, type Release } from "@/update/release";

/** Opens every release, newest first, each a fold of its notes with only
 * the newest open. Nothing is fetched until it opens. */
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
        <Newspaper />
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
  // Counts the retries of a failed page, which refetch it.
  const [tries, setTries] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: fetches again on a retry
  useEffect(() => {
    let live = true;
    setStatus("loading");
    fetchReleases(askedRepo(), pages).then(
      ({ releases, more }) => {
        if (!live) return;
        setList((l) => [...l, ...releases]);
        setStatus(more ? "more" : "end");
      },
      (e: unknown) => {
        if (!live) return;
        setError(message(e));
        setStatus("error");
      },
    );
    return () => {
      live = false;
    };
  }, [pages, tries]);
  return (
    <div className="-mx-5 min-h-0 flex-1 overflow-auto border-line border-t px-5">
      <div className="flex flex-col gap-2 pt-4">
        {list.map((r, i) => (
          <ReleaseNotes key={r.version} release={r} newest={i === 0} />
        ))}
      </div>
      <div className="flex justify-center py-4">
        {status === "loading" && (
          <Loader2 className="size-4 animate-spin text-ink-muted" />
        )}
        {status === "more" && (
          <Button variant="ghost" size="sm" onClick={() => setPages(pages + 1)}>
            Load Older Releases
          </Button>
        )}
        {status === "end" && list.length === 0 && (
          <p className="text-ink-muted text-sm">No releases yet</p>
        )}
        {status === "error" && (
          <div className="flex flex-col items-center gap-2">
            <InlineError error={`Couldn't reach GitHub\n\n${error}`} />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setTries(tries + 1)}
            >
              Try Again
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}

/** Props for {@link ReleaseNotes}. */
interface ReleaseNotesProps {
  release: Release;
  /** Opens at the start. The rest start closed. */
  newest: boolean;
}

/** One release, a fold headed by its version and date. Its notes render
 * once it first opens. */
function ReleaseNotes({ release, newest }: ReleaseNotesProps) {
  const [current] = useUpdate.usePath("current");
  const [open, setOpen] = useState(newest);
  return (
    <Disclosure
      title={
        <>
          v{release.version}
          {release.version === current && (
            <Badge variant="info">Current Version</Badge>
          )}
        </>
      }
      meta={release.published && day(release.published)}
      open={open}
      onOpenChange={setOpen}
      lazy
    >
      <div className="pr-4 pb-4 pl-10">
        <ReleaseLink href={release.url}>Release Page</ReleaseLink>
        <Notes md={release.notes} className="pt-1.5" />
      </div>
    </Disclosure>
  );
}
