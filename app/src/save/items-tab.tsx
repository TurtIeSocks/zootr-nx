import { commands, type Intent, type ItemsView } from "@/bindings";
import { call } from "@/lib/api";
import { ItemsEditor } from "@/save/items-editor";
import { stage } from "@/save/tabs";
import { useUi } from "@/stores/ui";

/** The Save page's Items tab: the editor, its clicks and Status fields
 * staged on the slot. */
export function ItemsTab({ view }: { view: ItemsView }) {
  return (
    <ItemsEditor
      view={view}
      onClick={clickCell}
      onStatus={(edit) => stage([edit])}
    />
  );
}

/** Asks what a click on cell `at` changes, and stages it. */
async function clickCell(at: number, intent: Intent) {
  const { slot, presets, edits } = useUi.getState().save;
  if (slot === null) return;
  await stage(
    await call(commands.saveClick(slot, { presets, edits }, at, intent)),
  );
}
