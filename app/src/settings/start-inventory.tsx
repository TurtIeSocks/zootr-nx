import { Loader2 } from "lucide-react";
import {
  commands,
  type Intent,
  type SettingDef,
  type StartInventory as Start,
} from "@/bindings";
import { DumpError } from "@/components/needs-dump";
import { call } from "@/lib/api";
import type { Settings } from "@/lib/config";
import { startOf } from "@/lib/settings";
import { useLoad } from "@/lib/use-load";
import { ItemsEditor } from "@/save/items-editor";
import { changeSettings, useConfig } from "@/stores/config";

/** Props for {@link StartInventory}. */
export interface StartInventoryProps {
  def: SettingDef;
}

const loadItems = (value: Settings[string] | undefined) =>
  call(commands.startItems(startOf(value)));

/** The starting inventory as the Save page's grid and Status fields, on a
 * new game. Each click saves the setting. */
export function StartInventory({ def }: StartInventoryProps) {
  const value = useConfig((s) => s.config.settings[def.id]);
  const view = useLoad(loadItems, value, true);
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
    <ItemsEditor
      view={view.data}
      onClick={async (at: number, intent: Intent) =>
        save(await call(commands.startClick(now(), at, intent)))
      }
      onStatus={async (edit) =>
        save(await call(commands.startSet(now(), edit)))
      }
    />
  );
}
