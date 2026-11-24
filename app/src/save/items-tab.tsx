import { commands, type Intent, type ItemsView } from "@/bindings";
import { call } from "@/lib/api";
import { ItemsEditor } from "@/save/items-editor";
import { stage, stageFrom } from "@/save/tabs";

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
const clickCell = (at: number, intent: Intent) =>
  stageFrom((slot, pending) =>
    call(commands.saveClick(slot, pending, at, intent)),
  );
