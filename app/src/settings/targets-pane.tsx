import { save as pickFile } from "@tauri-apps/plugin-dialog";
import {
  Check,
  ClipboardCopy,
  Download,
  FolderInput,
  Plus,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { commands, type Target, type TargetStatus } from "@/bindings";
import { Callout } from "@/components/callout";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { PathInput } from "@/components/path-input";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { UnsavedBar } from "@/components/unsaved-bar";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { useCopied } from "@/lib/use-copied";
import { BuildNote } from "@/settings/build-note";
import { ImportTargetDialog } from "@/settings/import-target-dialog";
import {
  ADVANCED_FIELDS,
  BLANK,
  type Draft,
  type DraftField,
  FIELDS,
  importedDraft,
  isEdited,
  isSaved,
  namesVariable,
  problem,
  shown,
  toDraft,
  toTarget,
  VARIABLES,
  worksIn,
} from "@/settings/targets-draft";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig, useDevTools } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

/** The apps that run the game, edited together and saved at once. Saved
 * targets that change start the form again; a reload of the same ones keeps
 * its edits. */
export function TargetsPane() {
  const [targets] = useConfig.usePath("config.targets");
  // Kept here, since the save that sets them starts the form again.
  const [notes, setNotes] = useState<string[]>([]);
  return (
    <RequestProvider key={JSON.stringify(targets)}>
      <TargetsForm notes={notes} onSaved={setNotes} />
    </RequestProvider>
  );
}

/** Props for {@link TargetsForm}. */
interface TargetsFormProps {
  /** What the last save changed on its own. */
  notes: string[];
  onSaved: (notes: string[]) => void;
}

let lastKey = 0;

/** A draft with a key of its own, so its card keeps its state when one
 * before it goes. */
const keyed = (draft: Draft) => ({ key: ++lastKey, draft });

/** The saved targets as keyed drafts. */
const cardsOf = (targets: Target[]) => targets.map((t) => keyed(toDraft(t)));

/** The drafts, one card each, with Add and Import in the header, and a bar
 * with Save floating at the foot while they differ from the saved targets.
 * Copying a target and saving them all share the request, one at a time. */
function TargetsForm({ notes, onSaved }: TargetsFormProps) {
  const [targets] = useConfig.usePath("config.targets");
  const [cards, setCards] = useState(() => cardsOf(targets));
  const drafts = cards.map((c) => c.draft);
  /** Why the form couldn't be saved as it was. */
  const [invalid, setInvalid] = useState("");
  const { busy, error, run } = useRequest();

  const save = () => {
    const bad = problem(drafts);
    setInvalid(bad);
    if (bad) return;
    const next = drafts.map(toTarget);
    const picked = useConfig.getPath("config.picked");
    run(async () =>
      onSaved(
        await saveConfig({
          targets: next,
          picked: next.some((t) => t.name === picked)
            ? picked
            : (next[0]?.name ?? null),
        }),
      ),
    );
  };

  return (
    <>
      <PageHeader
        title="Targets"
        help="target"
        description="Where you play: what Play opens, and the folder the game loads mods from. Install writes the seed to every target."
        actions={
          <>
            <Button
              variant="secondary"
              onClick={() => setCards((cs) => [...cs, keyed(BLANK)])}
            >
              <Plus /> Add a Target
            </Button>
            <ImportTargetDialog
              onImport={(t) =>
                setCards((cs) => [
                  ...cs,
                  keyed(
                    importedDraft(
                      cs.map((c) => c.draft),
                      t,
                    ),
                  ),
                ])
              }
            />
          </>
        }
      />
      {notes.map((note) => (
        <Callout key={note} tone="info" icon={FolderInput}>
          {note}
        </Callout>
      ))}
      {cards.map(({ key, draft }) => (
        <TargetCard
          key={key}
          id={String(key)}
          draft={draft}
          onChange={(patch) =>
            setCards((cs) =>
              cs.map((c) =>
                c.key === key ? { key, draft: { ...c.draft, ...patch } } : c,
              ),
            )
          }
          onRemove={() => setCards((cs) => cs.filter((c) => c.key !== key))}
        />
      ))}
      <VariablesCard open={drafts.some(namesVariable)} />
      {isEdited(targets, drafts) && (
        <UnsavedBar
          message="Unsaved changes to targets"
          error={invalid || error}
          busy={busy}
          saveLabel="Save Targets"
          onDiscard={() => {
            setInvalid("");
            setCards(cardsOf(targets));
          }}
          onSave={save}
        />
      )}
    </>
  );
}

/** Every variable a field can name, at the foot of the page: open while
 * a target names one. */
function VariablesCard({ open }: { open: boolean }) {
  const devTools = useDevTools();
  // Read once, so typing a variable doesn't spring it open.
  const [initial] = useState(open);
  return (
    <Card>
      <details open={initial} className="px-4 py-3 text-[13px]">
        <summary className="cursor-pointer font-semibold text-[15px] text-ink">
          Variables
        </summary>
        <table className="mt-3 w-full text-left">
          <thead className="text-ink-faint text-xs">
            <tr>
              <th className="pb-1.5 font-medium">Variable</th>
              <th className="pb-1.5 font-medium">Works in</th>
              <th className="pb-1.5 font-medium">Becomes</th>
            </tr>
          </thead>
          <tbody>
            {shown(VARIABLES, devTools).map((v) => (
              <tr key={v.name} className="border-line border-t align-top">
                <td className="type-mono py-1.5 pr-4">{v.name}</td>
                <td className="py-1.5 pr-4 text-ink-muted">
                  {worksIn(v, devTools)}
                </td>
                <td className="py-1.5 text-ink-muted">{v.becomes}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-3 text-ink-muted">
          A ~ at the start of a path becomes your home folder, so a target
          shared from another computer works on yours.
        </p>
      </details>
    </Card>
  );
}

/** Props for {@link TargetCard}. */
interface TargetCardProps {
  /** Unique on the page, for the field ids. */
  id: string;
  draft: Draft;
  onChange: (patch: Partial<Draft>) => void;
  onRemove: () => void;
}

/** One target's fields, with Copy and Download once it's saved, and Remove. */
function TargetCard({ id, draft, onChange, onRemove }: TargetCardProps) {
  const devTools = useDevTools();
  const advanced = shown(ADVANCED_FIELDS, devTools);
  const [targets] = useConfig.usePath("config.targets");
  const { run } = useRequest();
  const { copied, flash } = useCopied();
  const saved = isSaved(targets, draft);
  const name = draft.name.trim();
  const title = draft.name || "New Target";
  const found = useSeeds((s) => s.status.targets.find((t) => t.name === name));
  // Read once, so clearing the fields doesn't close the section on them.
  const [advancedOpen] = useState(() =>
    advanced.some((f) => draft[f.key] !== ""),
  );
  const copy = () =>
    run(async () => {
      await navigator.clipboard.writeText(
        await call(commands.exportTarget(name)),
      );
      flash("target");
    });
  const download = () =>
    run(async () => {
      const file = await pickFile({
        defaultPath: `${name}.json`,
        filters: [{ name: "Target", extensions: ["json"] }],
      });
      if (file) await call(commands.saveTarget(name, file));
    });
  const unsaved = "Save this target to share it";

  return (
    <Card
      title={draft.release ? `${title} (${draft.release})` : title}
      action={
        <>
          <TipButton
            label={`Copy ${title}`}
            tip={
              !saved ? unsaved : copied ? "Copied" : "Copy this target to share"
            }
            variant="ghost"
            disabled={!saved}
            onClick={copy}
          >
            {copied ? <Check /> : <ClipboardCopy />}
          </TipButton>
          <TipButton
            label={`Download ${title}`}
            tip={saved ? "Save this target to a file to share" : unsaved}
            variant="ghost"
            disabled={!saved}
            onClick={download}
          >
            <Download />
          </TipButton>
          <TipButton
            label={`Remove ${title}`}
            tip="Remove this target"
            variant="ghost"
            onClick={onRemove}
          >
            <Trash2 />
          </TipButton>
        </>
      }
    >
      <div className="grid gap-4 px-4 pt-2 pb-4">
        <BuildNote name={name} />
        {FIELDS.map((field) => (
          <TargetField
            key={field.key}
            id={`${id}-${field.key}`}
            field={field}
            value={draft[field.key]}
            warn={saved ? missing(found, field.key) : undefined}
            onChange={(value) => onChange({ [field.key]: value })}
          />
        ))}
        <details className="text-[13px]" open={advancedOpen}>
          <summary className="cursor-pointer text-ink-muted">Advanced</summary>
          <div className="grid gap-4 pt-3">
            {advanced.map((field) => (
              <TargetField
                key={field.key}
                id={`${id}-${field.key}`}
                field={field}
                value={draft[field.key]}
                onChange={(value) => onChange({ [field.key]: value })}
              />
            ))}
          </div>
        </details>
      </div>
    </Card>
  );
}

/** What a saved target's `key` field names that status didn't find, if
 * anything. */
function missing(found: TargetStatus | undefined, key: string) {
  if (!found) return undefined;
  if (key === "app" && !found.app) return "App not found";
  if (key === "mod_dir" && !found.mod_dir) return "Mod folder not found";
  return undefined;
}

/** Props for {@link TargetField}. */
interface TargetFieldProps {
  id: string;
  field: DraftField;
  value: string;
  /** Replaces the hint while set. */
  warn?: string;
  onChange: (value: string) => void;
}

/** A labelled field: a path with Browse, the settings lines, or a line of
 * text. */
export function TargetField({
  id,
  field,
  value,
  warn,
  onChange,
}: TargetFieldProps) {
  const { key, label, help, hint, pick, multiline } = field;
  return (
    <Field id={id} label={label} help={help} hint={hint} warn={warn}>
      {pick ? (
        <PathInput id={id} pick={pick} value={value} onChange={onChange} />
      ) : multiline ? (
        <Textarea
          id={id}
          className="type-mono"
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <Input
          id={id}
          className={key === "name" ? "" : "type-mono"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}
