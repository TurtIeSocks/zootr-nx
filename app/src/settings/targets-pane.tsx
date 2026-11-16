import { save as pickFile } from "@tauri-apps/plugin-dialog";
import { Check, ClipboardCopy, Download, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { commands } from "@/bindings";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { FloatingBar } from "@/components/floating-bar";
import { PathInput } from "@/components/path-input";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { useCopied } from "@/lib/use-copied";
import { ImportTargetDialog } from "@/settings/import-target-dialog";
import {
  BLANK,
  type Draft,
  type DraftField,
  FIELDS,
  isEdited,
  isSaved,
  problem,
  toDraft,
  toTarget,
  withTarget,
} from "@/settings/targets-draft";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig } from "@/stores/config";

/** The apps that run the game, edited together and saved at once. Saved
 * targets that change start the form again; a reload of the same ones keeps
 * its edits. */
export function TargetsPane() {
  const [targets] = useConfig.usePath("config.targets");
  return (
    <RequestProvider key={JSON.stringify(targets)}>
      <TargetsForm />
    </RequestProvider>
  );
}

/** The drafts, one card each, with Add and Import in the header, and a bar
 * with Save floating at the foot while they differ from the saved targets.
 * Copying a target and saving them all share the request, one at a time. */
function TargetsForm() {
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
    run(() =>
      saveConfig({
        targets: next,
        picked: next.some((t) => t.name === picked)
          ? picked
          : (next[0]?.name ?? null),
      }),
    );
  };

  return (
    <>
      <PageHeader
        title="Targets"
        description="A target is an app that runs the game with a mod folder. Install writes the seed to every target."
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
      {isEdited(targets, drafts) && (
        <FloatingBar
          message={
            invalid || error ? (
              <InlineError error={invalid || error} />
            ) : (
              <p>Unsaved changes to targets</p>
            )
          }
        >
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => {
              setInvalid("");
              setDrafts(targets.map(toDraft));
            }}
          >
            Discard
          </Button>
          <Button disabled={busy} onClick={save}>
            Save targets
          </Button>
        </FloatingBar>
      )}
    </>
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
  const [targets] = useConfig.usePath("config.targets");
  const { run } = useRequest();
  const { copied, flash } = useCopied();
  const saved = isSaved(targets, draft);
  const name = draft.name.trim();
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
        {FIELDS.map((field) => (
          <TargetField
            key={field.key}
            id={`${id}-${field.key}`}
            field={field}
            value={draft[field.key]}
            onChange={(value) => onChange({ [field.key]: value })}
          />
        ))}
      </div>
    </Card>
  );
}

/** Props for {@link TargetField}. */
interface TargetFieldProps {
  id: string;
  field: DraftField;
  value: string;
  onChange: (value: string) => void;
}

/** A labelled field: a path with Browse, the settings lines, or a line of
 * text. */
function TargetField({ id, field, value, onChange }: TargetFieldProps) {
  const { key, label, hint, pick } = field;
  return (
    <Field id={id} label={label}>
      {pick ? (
        <PathInput
          id={id}
          pick={pick}
          placeholder={hint}
          value={value}
          onChange={onChange}
        />
      ) : key === "lines" ? (
        <Textarea
          id={id}
          className="type-mono"
          placeholder={hint}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <Input
          id={id}
          className={key === "name" ? "" : "type-mono"}
          placeholder={hint}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}
