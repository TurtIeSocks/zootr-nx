import { NoSeed } from "@/components/no-seed";
import { CoveredSpoiler } from "@/play/spoiler/spoiler-sheet";
import { Page } from "@/shell/page-header";
import { useSeeds } from "@/stores/seeds";

/** The installed seed's spoiler, inline in a card the way Checks draws its
 * table, or where to make one when none is installed. */
export function SpoilerView() {
  const id = useSeeds((s) => s.history.installed?.id);
  if (id === undefined) return <NoSeed />;
  return (
    <Page wide className="min-h-0 flex-1">
      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
        <CoveredSpoiler id={id} />
      </section>
    </Page>
  );
}
