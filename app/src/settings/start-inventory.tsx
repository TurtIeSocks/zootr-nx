import { Loader2 } from "lucide-react";
import { useMemo } from "react";
import {
  commands,
  type Intent,
  type SettingDef,
  type StartInventory as Start,
} from "@/bindings";
import { DumpError } from "@/components/needs-dump";
import { call } from "@/lib/api";
import type { Settings } from "@/lib/config";
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
import { changeSettings, useConfig } from "@/stores/config";

/** Props for {@link StartInventory}. */
export interface StartInventoryProps {
  def: SettingDef;
}

/** What the grid loads: the start, the words each giver gives, and the
 * settings, whose bags change what it can hold. */
interface Load {
  start: Start;
  given: string[][];
  settings: Settings;
}

const loadItems = ({ start, given, settings }: Load) =>
  call(commands.startItems(start, given, settings));

/** `start` with the arsenal (all of Link's gear) added. */
const withArsenal = (start: Start) => call(commands.startArsenal(start));

/** What every new game holds whatever the settings, shown held and
 * locked like a setting's gift: Link can't start without his tunic. */
const NEW_GAME = { name: "every new game", words: ["kokiri-tunic"] };

/** Each setting that gives items, then {@link NEW_GAME}. */
const giversOf = (schema: SettingDef[], settings: Settings) => [
  ...startGivers(schema, settings),
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
  const settings = useConfig((s) => s.config.settings);
  const schema = useConfig((s) => s.schema);
  const value = settings[def.id];
  const forcers = useMemo(
    () => startForcers(schema, settings),
    [schema, settings],
  );
  const givers = useMemo(() => giversOf(schema, settings), [schema, settings]);
  /** The setting's own words, without what other settings force. */
  const start = useMemo(
    () => startOf(def, value === undefined ? {} : { [def.id]: value }),
    [def, value],
  );
  const load = useMemo(
    () => ({
      start: withForced(
        start,
        forcers.flatMap((f) => f.words),
      ),
      given: givers.map((g) => g.words),
      settings,
    }),
    [start, forcers, givers, settings],
  );
  const view = useLoad(loadItems, load, true);
  const armed = useLoad(withArsenal, start);
  /** The settings as they stand now, read at the moment of a change. */
  const now = () => useConfig.getState().config.settings;
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
        onPick={async (on) =>
          save(on ? await withArsenal(startOf(def, now())) : {})
        }
        already={ALREADY}
        tip={TIP}
        className="pl-0"
      />
      <ItemsEditor
        view={view.data}
        lockedBy={givers.map((g) => g.name)}
        onClick={async (at: number, intent: Intent) => {
          const current = now();
          await save(
            await call(
              commands.startClick(startOf(def, current), at, intent, current),
            ),
          );
        }}
      />
      {forcers.map(({ def: f }) => (
        <LockedBy key={f.id} parent={f} />
      ))}
    </div>
  );
}
