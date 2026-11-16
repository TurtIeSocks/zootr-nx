import { CoveredSpoiler } from "@/play/spoiler/spoiler-sheet";
import { Page } from "@/shell/page-header";
import { useSeeds } from "@/stores/seeds";

/** The installed seed's spoiler, inline in a card the way Checks draws its
 * table, or a line saying no seed is installed. */
export function SpoilerView() {
  const id = useSeeds((s) => s.history.installed?.id);
  if (id === undefined)
    return (
      <p className="py-12 text-center text-ink-muted">No seed is installed</p>
    );
  return (
    <Page wide className="min-h-0 flex-1">
      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
        <CoveredSpoiler id={id} />
      </section>
    </Page>
  );
}
