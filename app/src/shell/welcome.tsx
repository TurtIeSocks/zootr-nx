import { Loader2 } from "lucide-react";
import { useState } from "react";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Button } from "@/components/ui/button";
import { WhyButton } from "@/components/why-button";
import { MAC } from "@/lib/platform";
import { useRequestState } from "@/lib/request";
import { cn } from "@/lib/utils";
import { GameDump } from "@/settings/game-dump";
import { BLANK, FIELDS, problem, toTarget } from "@/settings/targets-draft";
import { TargetField } from "@/settings/targets-pane";
import { saveConfig, useConfig } from "@/stores/config";
import { useNoDump } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

/** The target fields the welcome asks for. The rest wait for Targets. */
const ASKED = FIELDS.filter((f) => ["name", "app", "mod_dir"].includes(f.key));

/** Ends the welcome for good, on Play. */
function finish() {
  useUi.setPath("welcome.done", true);
  useUi.setPath("tab", "Play");
}

/** The first launch's one screen: the game dump, then where you play.
 * Done saves the target once both are ready; Skip leaves both for later. */
export function Welcome() {
  const [game] = useConfig.usePath("config.game");
  const [draft, setDraft] = useState(BLANK);
  const save = useRequestState();
  // Each missing thing, one to a line in the tip.
  const why = [useNoDump(), problem([draft])].filter(Boolean).join("\n");
  const done = () =>
    save.run(async () => {
      const t = toTarget(draft);
      await saveConfig({ targets: [t], picked: t.name });
      finish();
    });

  return (
    <div className="flex h-full flex-col bg-canvas">
      <div
        data-tauri-drag-region
        className={cn(
          "flex h-(--titlebar) shrink-0 items-center px-4",
          MAC ? "justify-end" : "justify-between",
        )}
      >
        <Button variant="ghost" size="sm" onClick={finish}>
          Skip
        </Button>
      </div>
      <main className="min-h-0 flex-1 overflow-auto px-8 pb-12">
        <div className="mx-auto flex max-w-xl flex-col gap-5">
          <div>
            <h1 className="type-display-lg text-ink">
              Welcome to zootr<span className="text-primary-ink">-nx</span>
            </h1>
            <p className="mt-1.5 text-[13px] text-ink-muted">
              Two things and you can play: your own copy of the game, and where
              you play it.
            </p>
          </div>
          <Card title="1. Choose your game dump">
            <div className="px-4 pt-2 pb-4">
              <GameDump key={game} />
            </div>
          </Card>
          <Card title="2. Add where you play">
            <div className="grid gap-4 px-4 pt-2 pb-4">
              {ASKED.map((field) => (
                <TargetField
                  key={field.key}
                  id={`welcome-${field.key}`}
                  field={field}
                  value={draft[field.key]}
                  onChange={(value) =>
                    setDraft((d) => ({ ...d, [field.key]: value }))
                  }
                />
              ))}
              <p className="text-ink-muted text-xs">
                The rest of a target's fields are on the Targets page.
              </p>
            </div>
          </Card>
          <InlineError error={save.error} />
          <div className="flex justify-end">
            <WhyButton size="lg" disabled={save.busy} why={why} onClick={done}>
              {save.busy && <Loader2 className="animate-spin" />}
              Done
            </WhyButton>
          </div>
        </div>
      </main>
    </div>
  );
}
