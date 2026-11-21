import { Check, Loader2 } from "lucide-react";
import { useState } from "react";
import { InlineError } from "@/components/callout";
import { Field } from "@/components/field";
import { PathInput } from "@/components/path-input";
import { Button } from "@/components/ui/button";
import { useRequestState } from "@/lib/request";
import { APP_FIELDS } from "@/settings/app-fields";
import { saveConfig, useConfig } from "@/stores/config";
import { useNoDump } from "@/stores/seeds";

/** The game dump's path, starting from the saved one, with Save, which
 * extracts it. Under it, whether the saved dump is ready, gone, or why
 * the last Save failed. Keyed by the saved path, it starts again on each
 * save. */
const FIELD = APP_FIELDS.game;

export function GameDump() {
  const [saved] = useConfig.usePath("config.game");
  const [game, setGame] = useState(saved ?? "");
  const save = useRequestState();
  return (
    <div className="flex flex-col gap-1.5">
      <Field
        id={FIELD.id}
        label={FIELD.label}
        help={FIELD.help}
        hint={FIELD.description}
      >
        <div className="flex gap-2">
          <PathInput
            id={FIELD.id}
            pick="file"
            extensions={["nup"]}
            placeholder="/path/to/oot.nup"
            value={game}
            onChange={setGame}
          />
          <Button
            onClick={() =>
              save.run(() => saveConfig({ game: game.trim() || null }))
            }
            disabled={save.busy || game === (saved ?? "")}
          >
            {save.busy && <Loader2 className="animate-spin" />}
            {save.busy ? "Extracting" : "Save"}
          </Button>
        </div>
      </Field>
      {save.error ? <InlineError error={save.error} /> : <Ready />}
    </div>
  );
}

/** Whether the saved dump is there to read: a tick once it is, or why
 * it isn't once one was saved. */
function Ready() {
  const saved = useConfig((s) => s.config.game !== null);
  const why = useNoDump();
  if (!saved) return null;
  if (why) return <InlineError error={why} />;
  return (
    <p className="flex items-center gap-1 text-primary-ink text-xs">
      <Check className="size-3.5" /> Ready, with the 1.1.1 update
    </p>
  );
}
