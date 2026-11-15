import { Badge } from "@/components/ui/badge";
import { ItemIcon } from "@/play/spoiler/icons";
import { PickButton } from "@/play/spoiler/pick-button";
import { heroAreas, itemOf, placeOf } from "@/play/spoiler/rows";
import { useSpoiler } from "@/play/spoiler/spoiler-context";

/** The checks Link can't reach Ganondorf without, by area, with their rounds. */
export function HeroTab() {
  const { spoiler } = useSpoiler();
  return (
    <div className="flex flex-col gap-4 px-5 py-4">
      {heroAreas(spoiler.spots).map((a) => (
        <section key={a.name}>
          <h3 className="mb-1 font-semibold text-[13.5px]">{a.name}</h3>
          <ul aria-label={a.name}>
            {a.spots.map((s) => (
              <li key={s.hash}>
                <PickButton
                  hash={s.hash}
                  className="flex h-8.5 w-full items-center gap-2 rounded-sm px-2 text-left text-[13px] hover:bg-surface-hover data-picked:bg-primary-soft"
                >
                  <span className="w-56 text-ink-muted">{placeOf(s)}</span>
                  <ItemIcon item={s.item} />
                  <span className="flex-1 font-medium">{itemOf(s)}</span>
                  {s.round != null && (
                    <Badge shape="square">Round {s.round}</Badge>
                  )}
                </PickButton>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
