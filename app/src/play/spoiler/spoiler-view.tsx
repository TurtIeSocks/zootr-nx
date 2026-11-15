import { EyeOff } from "lucide-react";
import { Button } from "@/components/ui/button";
import { SpoilerBody } from "@/play/spoiler/spoiler-sheet";
import { Page } from "@/shell/page-header";
import { useSeeds } from "@/stores/seeds";
import { useShell } from "@/stores/shell";

/** The installed seed's spoiler, inline in a card the way Checks draws its
 * table, or a line saying no seed is installed. While the game runs it
 * stays covered until revealed, once per seed. */
export function SpoilerView() {
  const id = useSeeds((s) => s.history.installed?.id);
  const [running] = useSeeds.usePath("running");
  const [revealed, setRevealed] = useShell.usePath("revealed");
  if (id === undefined)
    return (
      <p className="py-12 text-center text-ink-muted">No seed is installed</p>
    );
  return (
    <Page wide className="min-h-0 flex-1">
      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
        {running && revealed !== id ? (
          <Cover onReveal={() => setRevealed(id)} />
        ) : (
          <SpoilerBody key={id} id={id} />
        )}
      </section>
    </Page>
  );
}

/** Props for {@link Cover}. */
interface CoverProps {
  onReveal: () => void;
}

/** Hides the spoiler mid run, so a stray click doesn't give it away. */
function Cover({ onReveal }: CoverProps) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
      <EyeOff className="size-6 text-ink-faint" />
      <h2 className="font-semibold text-[15px]">Hidden while you play</h2>
      <p className="max-w-[44ch] text-[13px] text-ink-muted">
        The spoiler shows where every item is. Reveal it if you're stuck, or
        when you're done.
      </p>
      <Button onClick={onReveal}>Reveal spoiler</Button>
    </div>
  );
}
