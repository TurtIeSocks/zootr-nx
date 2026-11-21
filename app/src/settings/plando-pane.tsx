import { Check, Loader2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { commands, type PlandoCheck, type PlandoOptions } from "@/bindings";
import { ArmedButton } from "@/components/armed-button";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { NameField } from "@/components/name-field";
import { DumpError } from "@/components/needs-dump";
import { RequestError } from "@/components/request-error";
import { Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
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
import { call, message } from "@/lib/api";
import { count } from "@/lib/format";
import {
  checkLabels,
  clear,
  itemLabels,
  load,
  type Plando,
  place,
  remove,
  samePins,
  saveAs,
  savedAs,
  unplace,
  unsaved,
} from "@/lib/plando";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig, useDevTools } from "@/stores/config";

/** The plando draft the next seed gets, and the saved plandos to load into
 * it or keep it as. */
export function PlandoPane() {
  return (
    <RequestProvider>
      <PageHeader
        title="Plando"
        help="plando"
        description="Pin items to checks. The next seed you generate gets the pins on this page and shuffles everything else around them. Each change saves as you make it."
      />
      <Saved />
      <Draft />
      <RequestError />
    </RequestProvider>
  );
}

/** Saves a config change through the page's request. */
function useStore() {
  const { run } = useRequest();
  return (change: Parameters<typeof saveConfig>[0]) =>
    run(() => saveConfig(change));
}

/** The label of the setting `id`, from the schema. */
const useSettingLabel = () => {
  const schema = useConfig((s) => s.schema);
  return (id: string) => schema.find((d) => d.id === id)?.label ?? id;
};

/** Props for {@link TurnOn}. */
interface TurnOnProps {
  /** The toggle's id. */
  setting: string;
}

/** Turns a setting on, so the checks it deals can be pinned. */
function TurnOn({ setting }: TurnOnProps) {
  const label = useSettingLabel();
  const store = useStore();
  const { busy } = useRequest();
  return (
    <Button
      size="sm"
      variant="secondary"
      disabled={busy}
      onClick={() =>
        store((c) => ({ settings: { ...c.settings, [setting]: true } }))
      }
    >
      Turn on {label(setting)}
    </Button>
  );
}

/** The draft's pins. */
const usePins = () => useConfig((s) => s.config.place);

/** The saved plandos: pick one to load it, or keep the draft under a name. */
function Saved() {
  const [plandos] = useConfig.usePath("config.plandos");
  const pins = usePins();
  const shown = useConfig((s) => savedAs(s.config));
  const replacing = useConfig((s) => unsaved(s.config));
  const { busy } = useRequest();
  const store = useStore();
  /** The name Save keeps the draft as, and Delete drops. */
  const [name, setName] = useState(() => shown ?? "");
  /** The saved plando waiting on the replace dialog. */
  const [asking, setAsking] = useState<string | null>(null);
  const names = Object.keys(plandos).sort();
  const typed = name.trim();
  const kept = plandos[typed];
  const empty = Object.keys(pins).length === 0;
  const open = (picked: string) => {
    setAsking(null);
    setName(picked);
    void store(load(picked));
  };

  return (
    <Card>
      <div className="p-4">
        <Field
          id="plando-saved"
          label="Saved Plandos"
          hint="Picking one loads its pins below, replacing them. Save keeps the pins below under the name, replacing a saved plando with that name"
        >
          <Select
            value={shown ?? ""}
            disabled={busy || names.length === 0}
            onValueChange={(n) => (replacing ? setAsking(n) : open(n))}
          >
            <SelectTrigger id="plando-saved" className="w-full">
              <SelectValue
                placeholder={
                  names.length === 0
                    ? "None saved yet"
                    : empty
                      ? "Load a saved plando"
                      : "Unsaved pins"
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
              aria-label="Plando name"
              placeholder="Plando name"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <Button
              disabled={
                busy || !typed || empty || (!!kept && samePins(kept, pins))
              }
              onClick={() => store(saveAs(typed))}
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
                void store(remove(typed));
              }}
            >
              Delete
            </ArmedButton>
          </div>
          <p className="flex items-center gap-1 text-ink-muted text-xs">
            {empty ? (
              "No pins, so the next seed shuffles every check"
            ) : shown ? (
              <>
                <Check className="size-3.5" />
                Saved as {shown}
              </>
            ) : (
              "Not saved. The next seed still gets these pins"
            )}
          </p>
        </Field>
      </div>
      <ReplaceDialog
        picked={asking}
        pins={Object.keys(pins).length}
        onCancel={() => setAsking(null)}
        onReplace={open}
      />
    </Card>
  );
}

/** Props for {@link ReplaceDialog}. */
interface ReplaceDialogProps {
  /** The saved plando to load, or none while closed. */
  picked: string | null;
  /** How many unsaved pins loading drops. */
  pins: number;
  onCancel: () => void;
  onReplace: (picked: string) => void;
}

/** Asks before a saved plando replaces pins no saved plando keeps. */
function ReplaceDialog({
  picked,
  pins,
  onCancel,
  onReplace,
}: ReplaceDialogProps) {
  return (
    <Dialog open={picked !== null} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Replace the Unsaved Pins?</DialogTitle>
          <DialogDescription>
            Loading {picked} replaces the {count(pins, "pin")} below. Save them
            first to keep them.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="pt-1">
          <DialogClose asChild>
            <Button variant="ghost">Cancel</Button>
          </DialogClose>
          <Button
            variant="destructive"
            onClick={() => picked && onReplace(picked)}
          >
            Replace
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** What the draft can pick from with the current settings, kept while a
 * newer read is on its way so the table doesn't blink on each edit. */
function useOptions(plando: Plando) {
  const [settings] = useConfig.usePath("config.settings");
  const [game] = useConfig.usePath("config.game");
  const [options, setOptions] = useState<PlandoOptions>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Read again whenever the draft, the settings or the dump change.
    void [plando, settings, game];
    let live = true;
    setLoading(true);
    call(commands.plandoOptions())
      .then((o) => {
        if (!live) return;
        setOptions(o);
        setError(undefined);
      })
      .catch((e) => live && setError(message(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [plando, settings, game]);

  return { options, error, loading };
}

/** The draft: a row to pin an item, the deal's error, and what it pins. */
function Draft() {
  const pins = usePins();
  const { options, error, loading } = useOptions(pins);
  return (
    <>
      <Card>
        <div className="p-4">
          <AddRow options={options} loading={loading} />
        </div>
      </Card>
      <DumpError error={error ?? options?.error ?? undefined} />
      <Placements plando={pins} options={options} />
    </>
  );
}

/** Props for {@link AddRow}. */
interface AddRowProps {
  options?: PlandoOptions;
  loading: boolean;
}

/** Pick a check and an item, then Pin. Pinning a check already pinned
 * replaces its item. */
function AddRow({ options, loading }: AddRowProps) {
  const store = useStore();
  const { busy } = useRequest();
  const label = useSettingLabel();
  const [check, setCheck] = useState("");
  const [item, setItem] = useState("");
  const checks = useMemo(() => checkLabels(options?.checks ?? []), [options]);
  const items = useMemo(() => itemLabels(options?.items ?? []), [options]);
  const picked = checks.get(check.trim());
  const needs = picked?.needs;
  const hash = needs ? undefined : picked?.hash;
  const actor = items.get(item.trim())?.actor;
  const pin = () => {
    if (!hash || !actor) return;
    void store(place(hash, actor));
    setCheck("");
    setItem("");
  };
  const reading = loading && !options;
  return (
    <div className="flex flex-col gap-1.5">
      <p className="font-medium text-[13px] text-ink leading-[18px]">
        Pin an Item
      </p>
      <div className="flex gap-2">
        <NameField
          label="Check"
          placeholder={reading ? "Reading the game…" : "Check"}
          names={[...checks.keys()]}
          value={check}
          onChange={setCheck}
          onEnter={pin}
          note={(n) => {
            const off = checks.get(n)?.needs;
            return off ? `needs ${label(off)}` : undefined;
          }}
        />
        <NameField
          label="Item"
          names={[...items.keys()]}
          value={item}
          onChange={setItem}
          onEnter={pin}
        />
        <Button
          variant="secondary"
          disabled={busy || !hash || !actor}
          onClick={pin}
        >
          {reading && <Loader2 className="animate-spin" />}
          Pin
        </Button>
      </div>
      {needs ? (
        <div className="flex items-center gap-2 text-ink-muted text-xs">
          Your settings don't deal this check. {label(needs)} does
          <TurnOn setting={needs} />
        </div>
      ) : (
        <p className="text-ink-muted text-xs">
          {check.trim() && !hash
            ? "Not a check the game has. Pick one from the list"
            : item.trim() && !actor
              ? "Not an item the pool holds. Pick one from the list"
              : "Checks your settings leave out say which setting deals them"}
        </p>
      )}
    </div>
  );
}

/** Where a pinned check sorts: by area then place, unknown ones last. */
const sortKey = (check?: PlandoCheck) =>
  check ? `${check.area}\n${check.place}` : "\uffff";

/** Props for {@link Placements}. */
interface PlacementsProps {
  plando: Plando;
  options?: PlandoOptions;
}

/** What the plando pins, by area then check. A check these settings don't
 * deal says so. */
function Placements({ plando, options }: PlacementsProps) {
  const devTools = useDevTools();
  const store = useStore();
  const { busy } = useRequest();
  const setting = useSettingLabel();
  const checks = new Map(options?.checks.map((c) => [c.hash, c]));
  const items = new Map(options?.items.map((i) => [i.actor, i.name]));
  const rows = Object.entries(plando)
    .map(([hash, actor]) => ({
      hash,
      check: checks.get(hash),
      item: items.get(actor) ?? actor,
    }))
    .sort((a, b) => sortKey(a.check).localeCompare(sortKey(b.check)));

  return (
    <Card
      title="Pinned"
      meta={count(rows.length, "check")}
      action={
        <ArmedButton
          size="sm"
          verb="clear"
          disabled={busy || rows.length === 0}
          onAct={() => store(clear())}
        >
          Clear
        </ArmedButton>
      }
    >
      {rows.length === 0 ? (
        <p className="px-4 pb-4 text-ink-muted text-sm">
          Nothing pinned yet. Generate shuffles every check.
        </p>
      ) : (
        <table className="w-full border-separate border-spacing-0 text-[13px]">
          <thead className="text-left text-ink-faint text-xs">
            <tr className="[&>th]:border-line [&>th]:border-b [&>th]:px-3 [&>th]:py-2 [&>th]:font-medium [&>th]:first:pl-4">
              <th>Area</th>
              <th>Check</th>
              <th>Vanilla</th>
              <th>Pinned</th>
              <th>
                <span className="sr-only">Remove</span>
              </th>
            </tr>
          </thead>
          <tbody className="[&_td]:h-8.5 [&_td]:border-line [&_td]:border-b [&_td]:px-3 [&_td]:first:pl-4 [&_tr:last-child_td]:border-b-0">
            {rows.map(({ hash, check, item }) => {
              const label = check?.place ?? hash;
              return (
                <tr key={hash} className="hover:bg-surface-hover">
                  <td>{check?.area ?? "—"}</td>
                  <Tip tip={devTools && hash}>
                    <td>
                      {label}
                      {options && (!check || check.needs) && (
                        <Badge variant="gold" className="ml-2">
                          {check?.needs
                            ? `Needs ${setting(check.needs)}`
                            : "Not dealt with these settings"}
                        </Badge>
                      )}
                    </td>
                  </Tip>
                  <td className="text-ink-muted">{check?.vanilla ?? "—"}</td>
                  <td className="font-medium">{item}</td>
                  <td className="w-px">
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={`Remove ${label}`}
                      disabled={busy}
                      onClick={() => store(unplace(hash))}
                    >
                      <X />
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </Card>
  );
}
