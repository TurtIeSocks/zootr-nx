import { openUrl } from "@tauri-apps/plugin-opener";
import type { Update } from "@tauri-apps/plugin-updater";
import { ArrowUpRight, CircleArrowUp } from "lucide-react";
import { type MouseEvent, useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { skip, useUpdate } from "@/stores/update";
import { install, installable, type Progress } from "@/update/install";
import { notesHtml, type Release } from "@/update/release";

/** Markdown's elements, styled for the popover. */
const NOTES =
  "max-h-80 overflow-auto px-4 py-5 [&>:first-child]:mt-0 text-ink-muted text-sm leading-relaxed [&_a]:text-primary-ink [&_a]:underline [&_code]:font-mono [&_code]:text-[12px] [&_:is(h1,h2,h3,h4,h5,h6)]:mt-2 [&_:is(h1,h2,h3,h4,h5,h6)]:font-medium [&_:is(h1,h2,h3,h4,h5,h6)]:text-ink [&_li+li]:mt-1.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mt-1.5 [&_ul]:list-disc [&_ul]:pl-5";

/** Opens a link from the notes in the browser, never in the webview. */
function follow(e: MouseEvent) {
  const a = (e.target as Element).closest("a");
  if (!a) return;
  e.preventDefault();
  if (a.protocol === "https:") void openUrl(a.href);
}

/** Props for {@link Notes}. */
interface NotesProps {
  md: string;
}

/** A release's notes, sanitized by {@link notesHtml}. */
function Notes({ md }: NotesProps) {
  const html = useMemo(() => notesHtml(md), [md]);
  if (!html.trim()) return null;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: clicks bubble up from the links inside, which take focus and keys themselves.
    // biome-ignore lint/a11y/useKeyWithClickEvents: as above.
    <div
      className={NOTES}
      onClick={follow}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: notesHtml sanitizes with DOMPurify.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

const day = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });

/** Install & restart, once the updater can install `version`. Held while
 * a game runs: the restart would drop its bridge and tracker. */
function useInstall(version: string) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running] = useSeeds.usePath("running");
  useEffect(() => {
    let live = true;
    void installable(version).then((u) => live && setUpdate(u));
    return () => {
      live = false;
    };
  }, [version]);
  const go = () => {
    if (!update) return;
    setError(null);
    setProgress({ done: 0, total: null });
    install(update, setProgress).catch((e: unknown) => {
      setProgress(null);
      setError(String(e));
    });
  };
  return { update, progress, error, running, go };
}

type InstallState = ReturnType<typeof useInstall>;

/** The install's button, across from skip. */
function InstallButton({ update, progress, running, go }: InstallState) {
  if (!update) return null;
  return (
    <Button size="sm" disabled={running || progress !== null} onClick={go}>
      {progress ? "Installing…" : "Install & restart"}
    </Button>
  );
}

/** What holds the install back, how far it got, or why it failed. */
function InstallStatus({ update, progress, error, running }: InstallState) {
  if (!update) return null;
  const share = progress?.total ? progress.done / progress.total : null;
  return (
    <>
      {running && !progress && (
        <p className="text-ink-faint text-xs">Close the game to update</p>
      )}
      {progress && (
        <div
          role="progressbar"
          aria-label="Download"
          aria-valuenow={share === null ? undefined : Math.round(share * 100)}
          className="h-1 overflow-hidden rounded-full bg-surface-sunken"
        >
          <div
            className={`h-full bg-primary ${share === null ? "w-full animate-pulse" : ""}`}
            style={share === null ? undefined : { width: `${share * 100}%` }}
          />
        </div>
      )}
      {error && (
        <p className="text-danger-ink text-xs">
          The update failed: {error}. Try the release page.
        </p>
      )}
    </>
  );
}

/** Props for {@link ReleaseLink}. */
interface ReleaseLinkProps {
  href: string;
  children: string;
}

/** A link beside the release's name, opened in the browser. */
function ReleaseLink({ href, children }: ReleaseLinkProps) {
  return (
    <a
      href={href}
      className="inline-flex items-center gap-0.5 hover:text-ink hover:underline"
      onClick={(e) => {
        e.preventDefault();
        void openUrl(href);
      }}
    >
      {children}
      <ArrowUpRight className="size-3" />
    </a>
  );
}

/** Props for {@link ReleaseCard}. */
interface ReleaseCardProps {
  release: Release;
  onSkip: () => void;
}

/** The popover's content: what's out with links to its changelog and
 * release page, its notes, then skip and the install when the updater has
 * it. */
function ReleaseCard({ release, onSkip }: ReleaseCardProps) {
  const install = useInstall(release.version);
  return (
    <>
      <div className="flex items-start justify-between gap-3 border-line border-b px-4 py-2.5">
        <div>
          <p className="font-medium text-sm">zootr-nx v{release.version}</p>
          {release.published && (
            <p className="text-ink-faint text-xs">
              Published {day(release.published)}
            </p>
          )}
        </div>
        <div className="flex flex-col items-end gap-0.5 text-ink-faint text-xs">
          <ReleaseLink href={release.changelog}>Changelog</ReleaseLink>
          <ReleaseLink href={release.url}>Release page</ReleaseLink>
        </div>
      </div>
      <Notes md={release.notes} />
      <div className="flex flex-col gap-1.5 border-line border-t px-4 py-2.5">
        <div className="flex justify-between gap-2">
          <Button size="sm" variant="ghost" onClick={onSkip}>
            Skip this version
          </Button>
          <InstallButton {...install} />
        </div>
        <InstallStatus {...install} />
      </div>
    </>
  );
}

/** Says a newer release is out, at the foot of the sidebar. Absent while
 * there's none or it was skipped. */
export function UpdateAlert() {
  const [latest] = useUpdate.usePath("latest");
  const [skipped] = useUi.usePath("update.skipped");
  const [open, setOpen] = useState(false);
  if (!latest || latest.version === skipped) return null;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="soft" size="sm" className="w-full justify-start">
          <CircleArrowUp className="size-3.5" />
          Update available
          <span className="ml-auto font-mono">v{latest.version}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent side="right" align="end" className="w-96 p-0">
        <ReleaseCard
          release={latest}
          onSkip={() => {
            skip(latest.version);
            setOpen(false);
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
