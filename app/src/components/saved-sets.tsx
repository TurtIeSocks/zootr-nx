import { Check, Loader2 } from "lucide-react";
import { useState } from "react";
import { ArmedButton } from "@/components/armed-button";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { count } from "@/lib/format";
import { useRequest } from "@/lib/request";
import { type SetKinds, sameDraft, savedSets } from "@/lib/saved-sets";
import { useSaveConfig } from "@/settings/use-save-config";
import { useConfig } from "@/stores/config";

/** What a {@link SavedSets} card calls its sets and what they hold. */
export interface SetWords {
  /** The field's label: `Saved Plandos`. */
  label: string;
  hint: string;
  /** One set, lower case: `plando`. */
  noun: string;
  /** One thing a draft holds: `pin`. */
  unit: string;
  /** The note under an empty draft. */
  empty: string;
  /** The note under a draft no saved set keeps. */
  unsaved: string;
}

/** Props for {@link SavedSets}. */
interface SavedSetsProps {
  /** Which config draft the sets are kept from. */
  kind: keyof SetKinds;
  /** The select's id. */
  id: string;
  words: SetWords;
}

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** A draft's saved sets: pick one to load it, or keep the draft under a
 * name. */
export function SavedSets({ kind, id, words }: SavedSetsProps) {
  const sets = savedSets(kind);
  const draft = useConfig((s) => sets.draftOf(s.config));
  const saved = useConfig((s) => sets.setsOf(s.config));
  const shown = useConfig((s) => sets.savedAs(s.config));
  const replacing = useConfig((s) => sets.unsaved(s.config));
  const { busy } = useRequest();
  const store = useSaveConfig();
  /** The name Save keeps the draft as, and Delete drops. */
  const [name, setName] = useState(() => shown ?? "");
  /** The saved set waiting on the replace dialog. */
  const [asking, setAsking] = useState<string | null>(null);
  const names = Object.keys(saved).sort();
  const typed = name.trim();
  const kept = saved[typed];
  const size = sets.size(draft);
  const open = (picked: string) => {
    setAsking(null);
    setName(picked);
    void store(sets.load(picked));
  };
  const units = `${capital(words.unit)}s`;

  return (
    <Card>
      <div className="p-4">
        <Field id={id} label={words.label} hint={words.hint}>
          <Select
            value={shown ?? ""}
            disabled={busy || names.length === 0}
            onValueChange={(n) => (replacing ? setAsking(n) : open(n))}
          >
            <SelectTrigger id={id} className="w-full">
              <SelectValue
                placeholder={
                  names.length === 0
                    ? "None saved yet"
                    : size === 0
                      ? `Load a saved ${words.noun}`
                      : `Unsaved ${words.unit}s`
                }
              />
            </SelectTrigger>
            <SelectContent align="start">
              {names.map((n) => (
                <SelectItem key={n} value={n}>
                  {n}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className="flex gap-2">
            <Input
              aria-label={`${capital(words.noun)} name`}
              placeholder={`${capital(words.noun)} name`}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <Button
              disabled={
                busy ||
                !typed ||
                size === 0 ||
                (!!kept && sameDraft(kept, draft))
              }
              onClick={() => store(sets.saveAs(typed))}
            >
              {busy && <Loader2 className="animate-spin" />}
              Save
            </Button>
            <ArmedButton
              variant="secondary"
              verb="delete"
              disabled={busy || !kept}
              onAct={() => {
                setName("");
                void store(sets.remove(typed));
              }}
            >
              Delete
            </ArmedButton>
          </div>
          <p className="flex items-center gap-1 text-ink-muted text-xs">
            {size === 0 ? (
              words.empty
            ) : shown ? (
              <>
                <Check className="size-3.5" />
                Saved as {shown}
              </>
            ) : (
              words.unsaved
            )}
          </p>
        </Field>
      </div>
      <Dialog
        open={asking !== null}
        onOpenChange={(o) => !o && setAsking(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Replace the Unsaved {units}?</DialogTitle>
            <DialogDescription>
              Loading {asking} replaces the {count(size, words.unit)} below.
              Save them first to keep them.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="pt-1">
            <DialogClose asChild>
              <Button variant="ghost">Cancel</Button>
            </DialogClose>
            <Button
              variant="destructive"
              onClick={() => asking && open(asking)}
            >
              Replace
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
