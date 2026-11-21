import { Tip } from "@/components/tip";
import { GivenAtStart, HeroStar } from "@/play/spoiler/icons";
import { PickButton } from "@/play/spoiler/pick-button";
import { areaOf, itemOf, placeOf, rule } from "@/play/spoiler/rows";
import { useSpoiler } from "@/play/spoiler/spoiler-context";

/** The picked check and its rule, each item it names linked to the round
 * and check that gives it. Nothing while no check is picked. */
export function Detail() {
  const { spoiler, picked } = useSpoiler();
  const spot = spoiler.spots.find((s) => s.hash === picked);
  if (!spot) return null;
  const parts = rule(spot, spoiler.spots, spoiler.start.items);
  return (
    <section
      aria-label="Check detail"
      className="border-line border-t bg-surface px-5 py-3 text-[13px]"
    >
      <p className="flex items-center gap-1.5 font-medium">
        {areaOf(spot)} · {placeOf(spot)} → {itemOf(spot)}
        {spot.given && <GivenAtStart />}
        {spot.hero && <HeroStar />}
      </p>
      <p className="mt-1 text-ink-muted">
        <span className="text-ink-faint">Needs: </span>
        {parts.length === 0
          ? "no rule, logic has no location here"
          : parts.map((p, i) =>
              "text" in p ? (
                // biome-ignore lint/suspicious/noArrayIndexKey: the parts never reorder
                <span key={i}>{p.text}</span>
              ) : p.giver ? (
                <Tip
                  // biome-ignore lint/suspicious/noArrayIndexKey: an item can repeat
                  key={i}
                  tip={`${areaOf(p.giver)} · ${placeOf(p.giver)}`}
                >
                  <PickButton
                    hash={p.giver.hash}
                    className="font-medium text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink"
                  >
                    {p.name}
                    {p.giver.round != null && ` (round ${p.giver.round})`}
                  </PickButton>
                </Tip>
              ) : (
                // biome-ignore lint/suspicious/noArrayIndexKey: an item can repeat
                <span key={i} className="font-medium text-ink">
                  {p.name}
                </span>
              ),
            )}
      </p>
    </section>
  );
}
