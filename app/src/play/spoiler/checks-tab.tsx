import { Search } from "lucide-react";
import { useMemo } from "react";
import { Tip } from "@/components/tip";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { GivenAtStart, HeroStar } from "@/play/spoiler/icons";
import { PickButton } from "@/play/spoiler/pick-button";
import {
  areaOf,
  itemOf,
  placeOf,
  rule,
  ruleText,
  search,
} from "@/play/spoiler/rows";
import { useSpoiler } from "@/play/spoiler/spoiler-context";
import { useDevTools } from "@/stores/config";
import { useUi } from "@/stores/ui";

/** Every check and what it holds, searchable, then the checks that hold
 * filler until logic can show a way in. */
export function ChecksTab() {
  const devTools = useDevTools();
  const {
    spoiler: { spots, unprovable, start },
    picked,
  } = useSpoiler();
  const [query, setQuery] = useUi.usePath("spoiler.query");
  const shown = search(spots, query);
  // Each check's rule, worked out once a spoiler rather than a keystroke.
  const texts = useMemo(
    () =>
      new Map(
        spots.map((s) => [s.hash, ruleText(rule(s, spots, start.items))]),
      ),
    [spots, start],
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-4 px-5 py-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" />
          <Input
            autoFocus
            className="pl-8"
            placeholder="Search areas and items"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <span className="text-[13px] text-ink-muted">
          {shown.length} of {spots.length} checks
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-line border-t">
        <table className="w-full border-separate border-spacing-0 text-[13px]">
          <thead className="sticky top-0 bg-surface-raised text-left text-ink-faint text-xs">
            <tr className="[&>th]:border-line [&>th]:border-b [&>th]:px-3 [&>th]:py-2 [&>th]:font-medium [&>th]:first:pl-5">
              <th>Area</th>
              <th>Chest</th>
              <th>Holds</th>
              <th>Round</th>
              <th>
                <span className="sr-only">Way of the Hero</span>
              </th>
              <th>Rule</th>
            </tr>
          </thead>
          <tbody className="[&_td]:h-8.5 [&_td]:border-line [&_td]:border-b [&_td]:px-3 [&_td]:first:pl-5">
            {shown.map((s) => {
              const text = texts.get(s.hash);
              return (
                <tr
                  key={s.hash}
                  className={cn(
                    "hover:bg-surface-hover",
                    picked === s.hash && "bg-primary-soft",
                  )}
                >
                  <td>{areaOf(s)}</td>
                  <Tip tip={devTools && `${s.holder} ${s.hash}`}>
                    <td>
                      <PickButton
                        hash={s.hash}
                        className="text-left text-ink-muted hover:text-ink"
                      >
                        {placeOf(s)}
                      </PickButton>
                    </td>
                  </Tip>
                  <td className="font-medium">
                    {itemOf(s)}
                    {s.given && <GivenAtStart />}
                  </td>
                  <td className="text-ink-muted tabular-nums">{s.round}</td>
                  <td>{s.hero && <HeroStar />}</td>
                  <Tip tip={text}>
                    <td className="max-w-80 truncate text-ink-muted">{text}</td>
                  </Tip>
                </tr>
              );
            })}
          </tbody>
        </table>
        {unprovable.length > 0 && (
          <section className="px-5 py-4 text-[13px]">
            <h3 className="mb-1 font-semibold">
              Filler only, until logic can show a way in
            </h3>
            <ul className="text-ink-muted">
              {unprovable.map((u) => (
                <li key={u}>{u}</li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
