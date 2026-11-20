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
import { commands, type TargetStatus } from "@/bindings";
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
  isEdited,
  isSaved,
  namesVariable,
  problem,
  shown,
  toDraft,
  toTarget,
  VARIABLES,
  withTarget,
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

/** The drafts, one card each, with Add and Import in the header, and a bar
 * with Save floating at the foot while they differ from the saved targets.
 * Copying a target and saving them all share the request, one at a time. */
function TargetsForm({ notes, onSaved }: TargetsFormProps) {
  const [targets] = useConfig.usePath("config.targets");
  const [drafts, setDrafts] = useState(() => targets.map(toDraft));
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
              onClick={() => setDrafts((ds) => [...ds, BLANK])}
            >
              <Plus /> Add a target
            </Button>
            <ImportTargetDialog
              onImport={(t) => setDrafts((ds) => withTarget(ds, t))}
            />
          </>
        }
      />
      {notes.map((note) => (
        <Callout key={note} tone="info" icon={FolderInput}>
          {note}
        </Callout>
      ))}
      {drafts.map((d, i) => (
        <TargetCard
          // biome-ignore lint/suspicious/noArrayIndexKey: drafts have no stable id until saved, and only grow at the end
          key={i}
          id={String(i)}
          draft={d}
          onChange={(patch) =>
            setDrafts((ds) =>
              ds.map((d, j) => (j === i ? { ...d, ...patch } : d)),
            )
          }
          onRemove={() => setDrafts((ds) => ds.filter((_, j) => j !== i))}
        />
      ))}
      <VariablesCard open={drafts.some(namesVariable)} />
      {isEdited(targets, drafts) && (
        <UnsavedBar
          message="Unsaved changes to targets"
          error={invalid || error}
          busy={busy}
          saveLabel="Save targets"
          onDiscard={() => {
            setInvalid("");
            setDrafts(targets.map(toDraft));
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
                <td className="py-1.5 pr-4 text-ink-muted">{v.in}</td>
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
      title={draft.name || "New target"}
      action={
        <>
          <TipButton
            label="Copy"
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
            label="Download"
            tip={saved ? "Save this target to a file to share" : unsaved}
            variant="ghost"
            disabled={!saved}
            onClick={download}
          >
            <Download />
          </TipButton>
          <TipButton
            label="Remove"
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
