import { useMemo } from "react";
import type { Start } from "@/bindings";
import { cn } from "@/lib/utils";
import { HeroStar, ItemIcon } from "@/play/spoiler/icons";
import { PickButton } from "@/play/spoiler/pick-button";
import {
  areaOf,
  type Card,
  columns,
  itemOf,
  type Need,
  needed,
  placeOf,
  unlocks,
} from "@/play/spoiler/rows";
import { useSpoiler } from "@/play/spoiler/spoiler-context";
import { useUi } from "@/stores/ui";

/** How a card relates to the picked one: it gives something the pick
 * needs, or one option of several, or the pick opens it up. */
type Glow = Need | "unlocked";

/** What screen readers hear after a glowing card's name. */
const NOTES: Record<Glow, string> = {
  all: "needed",
  any: "one option",
  unlocked: "unlocked",
};

/** What every card in a column shares. What the pick needs glows gold,
 * dashed when it's one option, and what it unlocks glows blue. */
const card =
  "w-full rounded-md border px-2.5 py-2 text-left text-[12.5px] transition-[background-color,border-color,box-shadow] duration-(--duration-fast) data-[glow=all]:border-gold data-[glow=all]:bg-gold-soft data-[glow=all]:ring-3 data-[glow=all]:ring-gold/35 data-[glow=any]:border-gold data-[glow=any]:border-dashed data-[glow=any]:bg-gold-soft data-[glow=unlocked]:border-info-ink data-[glow=unlocked]:bg-info-soft";

/** Says how a card relates to the pick, to screen readers only. */
function GlowNote({ glow }: { glow?: Glow }) {
  return glow && <span className="sr-only"> ({NOTES[glow]})</span>;
}

/** Props for {@link CardView}. */
interface CardViewProps {
  card: Card;
  glow?: Glow;
}

/** A source as a dashed note, or a check as a card that picks it. */
function CardView({ card: c, glow }: CardViewProps) {
  if (c.kind === "source")
    return (
      <div className={cn(card, "border-line border-dashed text-ink-muted")}>
        {c.name}
      </div>
    );
  const s = c.spot;
  return (
    <PickButton
      hash={s.hash}
      data-glow={glow}
      className={cn(
        card,
        "border-line bg-surface hover:bg-surface-hover data-picked:border-primary data-picked:bg-primary-soft",
      )}
    >
      <span className="flex items-center gap-1.5 font-medium">
        <ItemIcon item={s.item} />
        <span className="min-w-0 flex-1">
          {itemOf(s)}
          <GlowNote glow={glow} />
        </span>
        {s.hero && <HeroStar />}
      </span>
      <span className="mt-1 block text-ink-muted">
        {areaOf(s)} · {placeOf(s)}
      </span>
    </PickButton>
  );
}

/** Props for {@link StartColumn}. */
interface StartColumnProps {
  start: Start;
  /** How each card relates to the pick, by key. */
  glows: Map<string, Glow>;
}

/** What logic counts Link as having before round 1: his items, then the
 * settings its rules read. Seeds from before it was kept show none. */
function StartColumn({ start, glows }: StartColumnProps) {
  if (start.items.length === 0 && start.settings.length === 0) return null;
  return (
    <section className="w-52 shrink-0">
      <h3 className="mb-2 font-semibold text-[13px]">Start</h3>
      <ul aria-label="Start" className="flex flex-col gap-2">
        {start.items.map((h) => (
          <li
            key={h.item}
            data-glow={glows.get(h.item)}
            className={cn(card, "flex items-center gap-1.5 border-line")}
          >
            <ItemIcon item={h.item} />
            <span className="min-w-0 flex-1">
              {h.name}
              <GlowNote glow={glows.get(h.item)} />
            </span>
            {h.count > 1 && <span className="text-ink-muted">×{h.count}</span>}
          </li>
        ))}
        {start.settings.map((s) => (
          <li
            key={s}
            className={cn(card, "border-line border-dashed text-ink-muted")}
          >
            {s}
          </li>
        ))}
      </ul>
    </section>
  );
}

/** How each card relates to the picked one, by key. A card the pick both
 * needs and unlocks shows as needed. */
function useGlows(): Map<string, Glow> {
  const { spoiler, picked } = useSpoiler();
  return useMemo(() => {
    const { spots, start } = spoiler;
    const spot = spots.find((s) => s.hash === picked);
    if (!spot) return new Map();
    const glows = new Map<string, Glow>(
      [...unlocks(spot, spots, start.items)].map((h) => [h, "unlocked"]),
    );
    for (const [key, need] of needed(spot, spots, start.items))
      glows.set(key, need);
    return glows;
  }, [spoiler, picked]);
}

/** A column a round, a card a check or source it collects. The cards
 * related to the pick glow. */
export function SpheresTab() {
  const { spoiler } = useSpoiler();
  const [heroOnly] = useUi.usePath("spoiler.heroOnly");
  const glows = useGlows();
  return (
    <div className="flex min-h-full gap-3 px-5 py-4">
      <StartColumn start={spoiler.start} glows={glows} />
      {columns(spoiler, heroOnly).map((col) => {
        const name = `Round ${col.round}`;
        return (
          <section key={col.round} className="w-52 shrink-0">
            <h3 className="mb-2 font-semibold text-[13px]">{name}</h3>
            <ul aria-label={name} className="flex flex-col gap-2">
              {col.cards.map((c) => (
                <li key={c.kind === "check" ? c.spot.hash : c.name}>
                  <CardView
                    card={c}
                    glow={
                      c.kind === "check" ? glows.get(c.spot.hash) : undefined
                    }
                  />
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
