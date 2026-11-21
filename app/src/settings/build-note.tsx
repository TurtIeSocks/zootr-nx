import { CircleStop, Hammer, TriangleAlert } from "lucide-react";
import type { BuildStatus } from "@/bindings";
import { Callout, ErrorWords, InlineError } from "@/components/callout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { WhyButton } from "@/components/why-button";
import { errorParts } from "@/lib/api";
import { useRequestState } from "@/lib/request";
import { GenerateSteps } from "@/play/generate-steps";
import {
  rebuild,
  stale,
  stopGenerate,
  useBuild,
  useSeeds,
  useTargetRunning,
} from "@/stores/seeds";

type Kind = BuildStatus["kind"];

/** Each status's badge. */
const BADGES: Record<
  Kind,
  { label: string; variant: "green" | "gold" | "danger" }
> = {
  current: { label: "Build Current", variant: "green" },
  stale: { label: "Needs a Rebuild", variant: "gold" },
  refused: { label: "Build Refused", variant: "danger" },
  unrecorded: { label: "Build Too Old", variant: "gold" },
};

/** `text`, saying how to rebuild `byHand` before its details. */
function withHint(text: string, byHand: boolean) {
  const { said, details } = errorParts(text);
  const hint = byHand
    ? ". Rebuild it by hand, or give it a rebuild command below to rebuild it from here"
    : "";
  return [said + hint, details].filter(Boolean).join("\n\n");
}

/** A saved target's build status, for a game that bakes the hook patch in
 * when it's built: a badge, Rebuild when it has a command, what's wrong,
 * and the rebuild's steps while it runs and after. */
export function BuildNote({ name }: { name: string }) {
  const build = useBuild(name);
  const [rebuilt] = useSeeds.usePath("rebuilt");
  const [generating] = useSeeds.usePath("generating");
  const [stopping] = useSeeds.usePath("stopping");
  const request = useRequestState();
  const running = useTargetRunning(name);
  if (!build) return null;
  const { label, variant } = BADGES[build.status.kind];
  const byHand = stale(build) && !build.command;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Badge variant={variant}>{label}</Badge>
        {build.command && (
          <WhyButton
            size="sm"
            variant="secondary"
            disabled={generating || request.busy}
            why={running && `Close ${name} first`}
            onClick={() => request.run(() => rebuild(name))}
          >
            <Hammer /> Rebuild
          </WhyButton>
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
          <ErrorWords text={withHint(build.text, byHand)} />
        </Callout>
      )}
      {rebuilt === name && <GenerateSteps />}
      <InlineError error={request.error} />
    </div>
  );
}
