import { useState } from "react";
import { commands } from "@/bindings";
import { InlineError } from "@/components/callout";
import { ErrorBoundary } from "@/components/error-boundary";
import { type EdgeMark, ScrollEdges } from "@/components/scroll-edges";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Segmented } from "@/components/ui/segmented";
import { call } from "@/lib/api";
import { useLoad } from "@/lib/use-load";
import { ChecksTab } from "@/play/spoiler/checks-tab";
import { Detail } from "@/play/spoiler/detail";
import { HeroTab } from "@/play/spoiler/hero-tab";
import { hasLogic, type SpoilerTab } from "@/play/spoiler/rows";
import { SpheresTab } from "@/play/spoiler/spheres-tab";
import {
  SpoilerContext,
  useShownTab,
  useSpoiler,
} from "@/play/spoiler/spoiler-context";
import { closeSpoiler, useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** The tabs a seed with logic shows, in order. */
const TABS: { id: SpoilerTab; label: string }[] = [
  { id: "spheres", label: "Spheres" },
  { id: "hero", label: "Way of the Hero" },
  { id: "checks", label: "All checks" },
];

/** The tab bodies by tab. */
const PANELS: Record<SpoilerTab, () => React.JSX.Element> = {
  spheres: SpheresTab,
  hero: HeroTab,
  checks: ChecksTab,
};

/** Glowing cards scrolled out of view, as dots on the scroll edges. */
const GLOW_MARKS: EdgeMark[] = [
  {
    selector: "[data-glow=all],[data-glow=any]",
    label: "needed",
    className: "bg-gold",
  },
  {
    selector: "[data-glow=unlocked]",
    label: "unlocked",
    className: "bg-info-ink",
  },
];

/** Loads a seed's spoiler by its history id. */
const loadSpoiler = (id: number) => call(commands.spoiler(id));

/** A seed's spoiler from History, in a dialog. The installed seed's is the
 * Spoiler tab. */
export function SpoilerSheet() {
  const [entry] = useShell.usePath("spoiler");
  return (
    <Dialog
      open={entry !== undefined}
      onOpenChange={(open) => !open && closeSpoiler()}
    >
      <DialogContent className="flex h-[85vh] max-w-[min(1200px,calc(100%-4rem))] flex-col">
        <DialogHeader>
          <DialogTitle>Spoiler</DialogTitle>
          <DialogDescription>
            Seed <span className="font-mono text-ink">{entry?.seed}</span>
          </DialogDescription>
        </DialogHeader>
        <div className="-mx-5 -mb-5 flex min-h-0 flex-1 flex-col border-line border-t">
          {/* Another seed starts a fresh load and pick. */}
          {entry && (
            <ErrorBoundary label="The spoiler" resetKey={entry.id}>
              <SpoilerBody key={entry.id} id={entry.id} />
            </ErrorBoundary>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Props for {@link SpoilerBody}. */
interface SpoilerBodyProps {
  /** The seed's history id. */
  id: number;
}

/** A seed's spoiler: the rounds a playthrough collects things in, the Way
 * of the Hero, and every check with its rule. Loads it, then shows it with
 * its pick in context, edge to edge in whatever frame holds it. Key it by
 * `id`, so another seed starts a fresh load and pick. */
export function SpoilerBody({ id }: SpoilerBodyProps) {
  const loaded = useLoad(loadSpoiler, id);
  const [picked, pick] = useState<string>();
  if (loaded.status === "loading") return null;
  if (loaded.status === "error")
    return <InlineError error={loaded.error} className="px-5 py-3" />;
  return (
    <SpoilerContext value={{ spoiler: loaded.data, picked, pick }}>
      <div className="border-line border-b px-5 py-2.5">
        <TabBar />
      </div>
      <ScrollEdges marks={GLOW_MARKS}>
        <TabPanel />
      </ScrollEdges>
      <Detail />
    </SpoilerContext>
  );
}

/** The tabs, and Spheres' hero filter; or, for a seed without logic, why
 * only its checks show. */
function TabBar() {
  const { spoiler } = useSpoiler();
  const [tab, setTab] = useUi.usePath("spoiler.tab");
  const [heroOnly, setHeroOnly] = useUi.usePath("spoiler.heroOnly");
  if (!hasLogic(spoiler))
    return (
      <p className="text-[13px] text-ink-muted">
        This seed predates the rounds, the Way of the Hero and the rules, or
        logic didn't place it, so only its checks show.
      </p>
    );
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <Segmented options={TABS} value={tab} onValueChange={setTab} />
      {tab === "spheres" && (
        // biome-ignore lint/a11y/noLabelWithoutControl: the checkbox inside is the control
        <label className="ml-auto flex cursor-pointer items-center gap-2 text-[13px]">
          <Checkbox
            checked={heroOnly}
            onCheckedChange={(on) => setHeroOnly(on === true)}
          />
          Way of the Hero only
        </label>
      )}
    </div>
  );
}

/** The shown tab's body. */
function TabPanel() {
  const Panel = PANELS[useShownTab()];
  return <Panel />;
}
