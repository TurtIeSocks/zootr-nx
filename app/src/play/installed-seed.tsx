import { TriangleAlert } from "lucide-react";
import { Callout } from "@/components/callout";
import { Meter } from "@/components/meter";
import { RequestError } from "@/components/request-error";
import { SeedIcons } from "@/components/seed-icons";
import { Badge } from "@/components/ui/badge";
import { when } from "@/lib/format";
import { RequestProvider } from "@/lib/request-provider";
import { summaryParts } from "@/lib/settings";
import { ApClient } from "@/play/ap-client";
import { PlayButtonGroup } from "@/play/play-button-group";
import { ExtrasBadges, Name } from "@/play/seed";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

/** The hero: what Play will start, and the Play button, whose request
 * fails under it. */
export function InstalledSeed() {
  const [schema] = useConfig.usePath("schema");
  const [installed] = useSeeds.usePath("history.installed");
  const [targets] = useSeeds.usePath("status.targets");
  // Builds that bake the hook patch in and haven't seen the installed one.
  const builds = targets.flatMap((t) => t.build?.text ?? []);

  return (
    <RequestProvider>
      <section className="flex shrink-0 flex-col gap-5 rounded-xl border border-line bg-surface p-6 shadow-sm">
        <div className="flex flex-wrap items-center gap-4">
          <div className="min-w-0 flex-1">
            <div className="flex h-5 items-center gap-2">
              <h2 className="type-overline">Installed</h2>
              {installed && <ExtrasBadges entry={installed} />}
            </div>
            {installed ? (
              <>
                {installed.label.name ? (
                  <Name entry={installed} className="type-display-lg mt-1.5" />
                ) : (
                  <div className="mt-2.5">
                    <SeedIcons icons={installed.icons} />
                  </div>
                )}
                <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 text-[13px] text-ink-muted">
                  {installed.label.name && (
                    <SeedIcons icons={installed.icons} />
                  )}
                  <span>{when(installed.time)}</span>
                </div>
              </>
            ) : (
              <>
                <p className="type-display-lg mt-1.5 text-ink-faint">Vanilla</p>
                <p className="mt-1 text-[13px] text-ink-muted">
                  No mod installed. Generate a seed below or install one from
                  History.
                </p>
              </>
            )}
          </div>
          <div className="flex items-center gap-2">
            <PlayButtonGroup />
          </div>
        </div>
        {installed && (
          <div className="flex flex-wrap items-center justify-between gap-4 border-line border-t pt-4">
            <div className="flex flex-wrap gap-1.5">
              {summaryParts(schema, installed.settings).map((s) => (
                <Badge key={s}>{s}</Badge>
              ))}
            </div>
            <Meter value={installed.changed} max={installed.chests}>
              {installed.changed} of {installed.chests} chests changed
            </Meter>
          </div>
        )}
        {installed?.ap_slot && <ApClient entry={installed} />}
        {builds.map((text) => (
          <Callout key={text} tone="gold" icon={TriangleAlert}>
            {text}
          </Callout>
        ))}
        <RequestError />
      </section>
    </RequestProvider>
  );
}
