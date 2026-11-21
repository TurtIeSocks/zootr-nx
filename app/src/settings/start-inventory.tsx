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
import { startForcers, startOf, withForced } from "@/lib/settings";
import { useLoad } from "@/lib/use-load";
import { ItemsEditor } from "@/save/items-editor";
import { LockedBy } from "@/settings/setting-row";
import { changeSettings, useConfig } from "@/stores/config";

/** Props for {@link StartInventory}. */
export interface StartInventoryProps {
  def: SettingDef;
}

const loadItems = (start: Start) => call(commands.startItems(start));

/** The starting inventory as the Save page's grid and Status fields, on a
 * new game. Each click saves the setting. What a choice's pick gives shows
 * held, with the choice named under the grid. */
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
  const shownStart = useMemo(
    () => withForced(startOf(value), forced.split(" ").filter(Boolean)),
    [value, forced],
  );
  const view = useLoad(loadItems, shownStart, true);
  /** The setting as it stands now, read at the moment of a change. */
  const now = () => startOf(useConfig.getState().config.settings[def.id]);
  const save = async (next: Start) => {
    const empty = Object.keys(next).length === 0;
    await changeSettings({ [def.id]: empty ? undefined : next });
  };
  if (view.status === "error") return <DumpError error={view.error} />;
  if (view.status === "loading") {
    return <Loader2 className="size-4 animate-spin text-ink-muted" />;
  }
  return (
    <div className="flex flex-col gap-1">
      <ItemsEditor
        view={view.data}
        onClick={async (at: number, intent: Intent) =>
          save(await call(commands.startClick(now(), at, intent)))
        }
        onStatus={async (edit) =>
          save(await call(commands.startSet(now(), edit)))
        }
      />
      {forcers.map((f) => (
        <LockedBy key={f.id} parent={f} />
      ))}
    </div>
  );
}
