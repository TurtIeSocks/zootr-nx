import type { Update } from "@tauri-apps/plugin-updater";
import { CircleArrowUp } from "lucide-react";
import { useEffect, useState } from "react";
import { InlineError } from "@/components/callout";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { WhyButton } from "@/components/why-button";
import { CLOSE_GAME } from "@/lib/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { skip, useUpdate } from "@/stores/update";
import { install, installable, type Progress } from "@/update/install";
import { Notes, ReleaseLink } from "@/update/notes";
import { day, type Release } from "@/update/release";

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
    <WhyButton
      size="sm"
      disabled={progress !== null}
      why={running && CLOSE_GAME}
      onClick={go}
    >
      {progress ? "Installing…" : "Install & Restart"}
    </WhyButton>
  );
}

/** What holds the install back, how far it got, or why it failed. */
function InstallStatus({ update, progress, error }: InstallState) {
  if (!update) return null;
  const share = progress?.total ? progress.done / progress.total : null;
  return (
    <>
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
      <InlineError
        error={
          error
            ? `The update didn't install. Try again, or download it from the release page\n\n${error}`
            : undefined
        }
      />
    </>
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
          <ReleaseLink href={release.url}>Release Page</ReleaseLink>
        </div>
      </div>
      <Notes md={release.notes} className="max-h-80 overflow-auto px-4 py-5" />
      <div className="flex flex-col gap-1.5 border-line border-t px-4 py-2.5">
        <div className="flex justify-between gap-2">
          <Tip tip="Hides this version until the next release">
            <Button size="sm" variant="ghost" onClick={onSkip}>
              Skip This Version
            </Button>
          </Tip>
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
          Update Available
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
