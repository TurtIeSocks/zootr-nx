import { Loader2, X } from "lucide-react";
import { useState } from "react";
import { commands } from "@/bindings";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { RequestError } from "@/components/request-error";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Input } from "@/components/ui/input";
import { call, same } from "@/lib/api";
import { fullTest, NO_TEST, testOn } from "@/lib/config";
import { useGameOptions } from "@/lib/game-options";
import { useRequest, useRequestState } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { useCopied } from "@/lib/use-copied";
import {
  ItemsField,
  NearField,
  RowsField,
  SpawnField,
  TextInput,
} from "@/settings/test-fields";
import {
  fromForm,
  type TestForm,
  TestFormContext,
  toForm,
  useTestForm,
} from "@/settings/test-form";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig } from "@/stores/config";

/** Where a new game starts and what it holds. Generate uses it until it's
 * cleared, and each seed keeps its own copy. Starts from the saved setup;
 * remount it to start again. */
export function TestPane() {
  const [form, setForm] = useState(() =>
    toForm(useConfig.getPath("config.test")),
  );
  const options = useGameOptions();
  const set = (patch: Partial<TestForm>) =>
    setForm((f) => ({ ...f, ...patch }));

  return (
    <TestFormContext value={{ form, set, options }}>
      <RequestProvider>
        <PageHeader
          title="Test setup"
          description="Generate starts every new game here, holding these items, until you clear it. Seeds built with it show a test badge in the history."
        />
        <InlineError
          error={options.status === "error" ? options.error : undefined}
        />
        <SavedSetups />
        <Card>
          <div className="flex flex-col gap-5 p-4">
            <SpawnField />
            <NearField />
            <ItemsField />
            <RowsField
              name="defaults"
              label="New-game flags"
              noun="flag"
              placeholder="Name=value"
              hint="GameData values a new game starts with, on top of the presets"
            />
            <RowsField
              name="progress"
              label="Quest steps"
              noun="quest step"
              placeholder="SageOfSpirit"
              hint="Run in order. A new game runs them in its opening and Apply runs them live. The names are checked when you save"
              quest
            />
            <RowsField
              name="goals"
              label="Goals"
              noun="goal"
              placeholder="IsGet.<item>"
              hint="The dev panel ticks the setup once they all hold"
            />
            <Field
              id="test-expect"
              label="Expect"
              hint="What a flag can't show, for the dev panel"
            >
              <TextInput
                name="expect"
                id="test-expect"
                placeholder="Gold Rupee from Anju"
              />
            </Field>
          </div>
        </Card>
        <SaveBar />
        <RequestError />
      </RequestProvider>
    </TestFormContext>
  );
}

/** Saves the config through the pane's request. */
const useStore = () => {
  const { run } = useRequest();
  return (next: Parameters<typeof saveConfig>[0]) =>
    run(() => saveConfig(next));
};

/** The setups saved by name: click one to use it, or save the form as one. */
function SavedSetups() {
  const [current] = useConfig.usePath("config.test");
  const [savedTests] = useConfig.usePath("config.saved_tests");
  const { busy } = useRequest();
  const store = useStore();
  const { form } = useTestForm();
  /** The name to save the form as. */
  const [name, setName] = useState("");
  const draft = fromForm(form);
  const saved = Object.entries(savedTests).map(
    ([n, t]) => [n, fullTest(t)] as const,
  );
  const forget = (gone: string) =>
    store((c) => ({
      saved_tests: Object.fromEntries(
        Object.entries(c.saved_tests).filter(([n]) => n !== gone),
      ),
    }));

  return (
    <Card>
      <div className="p-4">
        <Field
          id="test-name"
          label="Saved setups"
          hint="Click one to use it. Save as keeps the fields below under a name, replacing one with the same name"
        >
          {saved.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {saved.map(([n, t]) => (
                <ButtonGroup key={n}>
                  <Button
                    size="sm"
                    variant={same(t, current) ? "selected" : "secondary"}
                    disabled={busy}
                    onClick={() => store({ test: t })}
                  >
                    {n}
                  </Button>
                  <Button
                    size="icon-sm"
                    variant="secondary"
                    aria-label={`Delete ${n}`}
                    disabled={busy}
                    onClick={() => forget(n)}
                  >
                    <X />
                  </Button>
                </ButtonGroup>
              ))}
            </div>
          )}
          <div className="flex gap-2">
            <Input
              id="test-name"
              placeholder="Q36 well"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <Button
              variant="secondary"
              disabled={busy || !name.trim() || !testOn(draft)}
              onClick={() =>
                store((c) => ({
                  test: draft,
                  saved_tests: { ...c.saved_tests, [name.trim()]: draft },
                }))
              }
            >
              Save as
            </Button>
          </div>
        </Field>
      </div>
    </Card>
  );
}

/** Save, Clear, and Copy as CLI once what's shown is saved. */
function SaveBar() {
  const [current] = useConfig.usePath("config.test");
  const { busy } = useRequest();
  const store = useStore();
  const { form, set } = useTestForm();
  const copy = useRequestState();
  const { copied, flash } = useCopied();
  const unchanged = same(fromForm(form), current);
  const clear = () => {
    set(toForm(NO_TEST));
    store({ test: NO_TEST });
  };

  return (
    <>
      <div className="flex gap-2">
        <Button
          onClick={() => store({ test: fromForm(form) })}
          disabled={busy || unchanged}
        >
          {busy && <Loader2 className="animate-spin" />}
          Save
        </Button>
        <Button
          variant="secondary"
          onClick={clear}
          disabled={busy || !testOn(current)}
        >
          Clear
        </Button>
        <Button
          variant="ghost"
          className="ml-auto"
          disabled={!unchanged || copy.busy}
          title={
            unchanged
              ? "The zootr shuffle command for your settings and this setup"
              : "Save first"
          }
          onClick={() =>
            copy.run(async () => {
              const line = await call(commands.shuffleCommand());
              await navigator.clipboard.writeText(line);
              flash("command");
            })
          }
        >
          {copied ? "Copied" : "Copy as CLI"}
        </Button>
      </div>
      <InlineError error={copy.error} />
    </>
  );
}
