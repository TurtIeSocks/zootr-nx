import { CircleStop, Hammer, TriangleAlert } from "lucide-react";
import type { BuildStatus } from "@/bindings";
import { Callout, InlineError } from "@/components/callout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useRequestState } from "@/lib/request";
import { GenerateSteps } from "@/play/generate-steps";
import {
  rebuild,
  stale,
  stopGenerate,
  useBuild,
  useSeeds,
} from "@/stores/seeds";

type Kind = BuildStatus["kind"];

/** Each status's badge. */
const BADGES: Record<
  Kind,
  { label: string; variant: "green" | "gold" | "danger" }
> = {
  current: { label: "Build current", variant: "green" },
  stale: { label: "Needs a rebuild", variant: "gold" },
  refused: { label: "Hook patch refused", variant: "danger" },
  unrecorded: { label: "Patches unrecorded", variant: "gold" },
};

/** A saved target's build status, for a game that bakes the hook patch in
 * when it's built: a badge, Rebuild when it has a command, what's wrong,
 * and the rebuild's steps while it runs and after. */
export function BuildNote({ name }: { name: string }) {
  const build = useBuild(name);
  const [rebuilt] = useSeeds.usePath("rebuilt");
  const [generating] = useSeeds.usePath("generating");
  const [stopping] = useSeeds.usePath("stopping");
  const request = useRequestState();
  if (!build) return null;
  const { label, variant } = BADGES[build.status.kind];
  const byHand = stale(build) && !build.command;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Badge variant={variant}>{label}</Badge>
        {build.command && (
          <Button
            size="sm"
            variant="secondary"
            disabled={generating || request.busy}
            onClick={() => request.run(() => rebuild(name))}
          >
            <Hammer /> Rebuild
          </Button>
        )}
        {generating && rebuilt === name && (
          <Button
            size="sm"
            variant="ghost"
            disabled={stopping}
            onClick={() => void stopGenerate()}
          >
            <CircleStop /> {stopping ? "Stopping" : "Stop"}
          </Button>
        )}
      </div>
      {build.text && (
        <Callout
          tone={build.status.kind === "refused" ? "danger" : "gold"}
          icon={TriangleAlert}
        >
          {build.text}
          {byHand &&
            ". Rebuild it by hand, or give it a rebuild command below to rebuild it from here"}
        </Callout>
      )}
      {rebuilt === name && <GenerateSteps />}
      <InlineError error={request.error} />
    </div>
  );
}
