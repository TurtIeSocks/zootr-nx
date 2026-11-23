import { Loader2 } from "lucide-react";
import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  commands,
  type Intent,
  type SettingDef,
  type StartInventory as Start,
} from "@/bindings";
import { DumpError } from "@/components/needs-dump";
import { call } from "@/lib/api";
import {
  sameStart,
  startForcers,
  startGivers,
  startOf,
  withForced,
} from "@/lib/settings";
import { useLoad } from "@/lib/use-load";
import { ItemsEditor } from "@/save/items-editor";
import { AllNone } from "@/settings/checklist";
import { LockedBy } from "@/settings/setting-row";
import { type ConfigState, changeSettings, useConfig } from "@/stores/config";

/** Props for {@link StartInventory}. */
export interface StartInventoryProps {
  def: SettingDef;
}

/** What the grid loads: the start, and the words each giver gives. */
interface Load {
  start: Start;
  given: string[][];
}

const loadItems = ({ start, given }: Load) =>
  call(commands.startItems(start, given));

/** `start` with the arsenal (all of Link's gear) added. */
const withArsenal = (start: Start) => call(commands.startArsenal(start));

/** What every new game holds whatever the settings, shown held and
 * locked like a setting's gift: Link can't start without his tunic. */
const NEW_GAME = { name: "every new game", words: ["kokiri-tunic"] };

/** Each setting that gives items, then {@link NEW_GAME}. */
const giversOf = (s: ConfigState) => [
  ...startGivers(s.schema, s.config.settings),
  NEW_GAME,
];

/** What All and None say when they'd change nothing, and what each does. */
const ALREADY = (on: boolean) =>
  on
    ? "Link already starts with the full arsenal"
    : "Link already starts empty";
const TIP = (on: boolean) =>
  on
    ? "Every piece of gear at its top tier, full ammo, every song, every trade item and twenty hearts"
    : "Start with nothing";

/** The starting inventory as the Save page's grid, on a new game, with
 * cells of its own for heart containers and rupees. Each click saves the
 * setting. What a choice's pick gives shows
 * held, with the choice named under the grid. What a setting gives by its
 * own path shows held and locked, naming the setting, and stays out of the
 * setting's count. */
export function StartInventory({ def }: StartInventoryProps) {
  const value = useConfig((s) => s.config.settings[def.id]);
  const forcers = useConfig(
    useShallow((s) =>
      startForcers(s.schema, s.config.settings).map((f) => f.def),
    ),
  );
  const forced = useConfig((s) =>
    startForcers(s.schema, s.config.settings)
      .flatMap((f) => f.words)
      .join(" "),
  );
  const givers = useConfig(useShallow((s) => giversOf(s).map((g) => g.name)));
  const given = useConfig((s) =>
    JSON.stringify(giversOf(s).map((g) => g.words)),
  );
  /** The setting's own words, without what other settings force. */
  const start = useMemo(
    () => startOf(def, value === undefined ? {} : { [def.id]: value }),
    [def, value],
  );
  const load = useMemo(
    () => ({
      start: withForced(start, forced.split(" ").filter(Boolean)),
      given: JSON.parse(given) as string[][],
    }),
    [start, forced, given],
  );
  const view = useLoad(loadItems, load, true);
  const armed = useLoad(withArsenal, start);
  /** The setting as it stands now, read at the moment of a change. */
  const now = () => startOf(def, useConfig.getState().config.settings);
  /** Saves `next`, or unsets it at the default. Emptied, it saves `{}`,
   * since unset is the default. */
  const save = async (next: Start) => {
    const kept =
      def.kind.type === "inventory" && sameStart(next, def.kind.default);
    await changeSettings({ [def.id]: kept ? undefined : next });
  };
  if (view.status === "error") return <DumpError error={view.error} />;
  if (view.status === "loading") {
    return <Loader2 className="size-4 animate-spin text-ink-muted" />;
  }
  return (
    <div className="flex flex-col gap-1">
      <AllNone
        of={def.label}
        would={(on) =>
          on
            ? armed.status !== "ok" || !sameStart(armed.data, start)
            : Object.keys(start).length > 0
        }
        onPick={async (on) => save(on ? await withArsenal(now()) : {})}
        already={ALREADY}
        tip={TIP}
        className="pl-0"
      />
      <ItemsEditor
        view={view.data}
        lockedBy={givers}
        onClick={async (at: number, intent: Intent) =>
          save(await call(commands.startClick(now(), at, intent)))
        }
      />
      {forcers.map((f) => (
        <LockedBy key={f.id} parent={f} />
      ))}
    </div>
  );
}
