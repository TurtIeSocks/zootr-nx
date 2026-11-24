import { CircleCheck, Loader2, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  commands,
  type FlagQuery,
  type Pending,
  type SaveEdit,
  type SlotRow,
} from "@/bindings";
import { Callout, InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { linkClass } from "@/components/styles";
import { Segmented } from "@/components/ui/segmented";
import { UnsavedBar } from "@/components/unsaved-bar";
import { call } from "@/lib/api";
import { CLOSE_GAME } from "@/lib/config";
import { count } from "@/lib/format";
import { useRequestState } from "@/lib/request";
import { type Loaded, useLoad } from "@/lib/use-load";
import { FlagsTab } from "@/save/flags-tab";
import { ItemsTab } from "@/save/items-tab";
import { PresetsTab } from "@/save/presets-tab";
import { SlotList } from "@/save/slot-list";
import {
  discard,
  pickSlot,
  type SaveTab,
  usePending,
  usePendingCount,
} from "@/save/tabs";
import { Page, PageHeader } from "@/shell/page-header";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

const TABS: { id: SaveTab; label: string }[] = [
  { id: "items", label: "Inventory" },
  { id: "flags", label: "Flags" },
  { id: "presets", label: "Presets" },
];

/** What a view of the slot is read with. `version` reads it again. */
interface ViewKey {
  slot: number;
  pending: Pending;
  version: number;
}

const loadSlots = (_version: number) => call(commands.saveSlots());
const loadItems = ({ slot, pending }: ViewKey) =>
  call(commands.saveItems(slot, pending));
const loadFlags = ({ slot, pending, query }: ViewKey & { query: FlagQuery }) =>
  call(commands.saveFlags(slot, pending, query));
const loadPresets = (_: null) => commands.savePresets();

/** A save slot's items, status and flags, edited and saved at once. */
export function SaveView() {
  const [version, setVersion] = useState(0);
  const slots = useLoad(loadSlots, version, true);
  const [slot] = useUi.usePath("save.slot");
  const [done, setDone] = useState<SaveEdit>();
  const reload = () => setVersion((v) => v + 1);
  useFollowSlots(slots);
  return (
    <Page wide>
      <PageHeader
        title="Save Editor"
        help="save-editor"
        description="Changes a save you already have: items, hearts and the rest, and any flag. Close the game first. The old save is kept next to it as a backup."
      />
      <Loading loaded={slots}>
        {(rows) => (
          <Card>
            <div className="flex flex-col gap-2 p-3">
              <SlotList rows={rows} />
            </div>
          </Card>
        )}
      </Loading>
      {done && <Saved done={done} />}
      {slot !== null && <SlotTabs slot={slot} version={version} />}
      <SaveBar
        onSaved={(edit) => {
          setDone(edit);
          reload();
        }}
        onReload={reload}
      />
    </Page>
  );
}

/** Picks the newest slot when none is, or the picked one is gone, and
 * takes the picked slot's stamp once it's read again after a save. */
function useFollowSlots(slots: Loaded<SlotRow[]>) {
  const [slot] = useUi.usePath("save.slot");
  const [stamp, setStamp] = useUi.usePath("save.stamp");
  useEffect(() => {
    if (slots.status !== "ok") return;
    const row = slots.data.find((r) => r.slot === slot);
    const first = slots.data[0];
    if (!row) {
      if (first) pickSlot(first.slot, first.stamp);
    } else if (stamp === null) {
      setStamp(row.stamp);
    }
  }, [slots, slot, stamp, setStamp]);
}

/** The inner tabs, each a view of the slot as Save would leave it. */
function SlotTabs({ slot, version }: { slot: number; version: number }) {
  const [tab, setTab] = useUi.usePath("save.tab");
  const pending = usePending();
  const key = useMemo(
    () => ({ slot, pending, version }),
    [slot, pending, version],
  );
  return (
    <div className="flex flex-col gap-3">
      <Segmented
        aria-label="View"
        className="self-start"
        options={TABS}
        value={tab}
        onValueChange={setTab}
      />
      {tab === "items" && <ItemsLoader viewKey={key} />}
      {tab === "flags" && <FlagsLoader viewKey={key} />}
      {tab === "presets" && <PresetsLoader />}
    </div>
  );
}

function ItemsLoader({ viewKey }: { viewKey: ViewKey }) {
  const items = useLoad(loadItems, viewKey, true);
  return <Loading loaded={items}>{(v) => <ItemsTab view={v} />}</Loading>;
}

function FlagsLoader({ viewKey }: { viewKey: ViewKey }) {
  const [query] = useUi.usePath("save.flags");
  const key = useMemo(() => ({ ...viewKey, query }), [viewKey, query]);
  const flags = useLoad(loadFlags, key, true);
  return <Loading loaded={flags}>{(v) => <FlagsTab view={v} />}</Loading>;
}

function PresetsLoader() {
  const presets = useLoad(loadPresets, null);
  return (
    <Loading loaded={presets}>{(p) => <PresetsTab presets={p} />}</Loading>
  );
}

/** Props for {@link Loading}. */
interface LoadingProps<T> {
  loaded: Loaded<T>;
  children: (data: T) => React.ReactNode;
}

/** A load's data once in, why it failed, or a spinner. */
function Loading<T>({ loaded, children }: LoadingProps<T>) {
  if (loaded.status === "ok") return children(loaded.data);
  if (loaded.status === "error") return <InlineError error={loaded.error} />;
  return <Loader2 className="size-4 animate-spin text-ink-muted" />;
}

/** What the last save changed and where the old file went. */
function Saved({ done }: { done: SaveEdit }) {
  return (
    <Callout tone="green" icon={CircleCheck}>
      <details>
        <summary className="cursor-pointer">
          Saved {count(done.log.length, "change")}. Backup at {done.backup}
        </summary>
        <pre className="type-mono mt-2 max-h-60 overflow-auto">
          {done.log.join("\n")}
        </pre>
      </details>
    </Callout>
  );
}

/** Props for {@link SaveBar}. */
interface SaveBarProps {
  onSaved: (edit: SaveEdit) => void;
  /** Reads the slot again. */
  onReload: () => void;
}

/** What's pending, Discard and Save. Saving over a file the game wrote
 * since it was read asks to reload first. */
function SaveBar({ onSaved, onReload }: SaveBarProps) {
  const pending = usePending();
  const [slot] = useUi.usePath("save.slot");
  const [stamp, setStamp] = useUi.usePath("save.stamp");
  const save = useRequestState();
  const [stale, setStale] = useState<number>();
  const n = usePendingCount();
  const [running] = useSeeds.usePath("running");
  // Nothing pending (a save, Discard, a slot switch) ends what it warned of.
  useEffect(() => {
    if (n === 0) setStale(undefined);
  }, [n]);
  if (n === 0 || slot === null) return null;
  const write = () =>
    save.run(async () => {
      const rows = await call(commands.saveSlots());
      const now = rows.find((r) => r.slot === slot)?.stamp;
      if (now !== undefined && now !== stamp) return setStale(now);
      const edit = await call(commands.saveWrite(slot, pending, stamp ?? 0));
      // The write moved the file's stamp: take it, so the next save here
      // doesn't read as the game's.
      const after = await call(commands.saveSlots());
      discard();
      setStamp(after.find((r) => r.slot === slot)?.stamp ?? null);
      onSaved(edit);
    });
  return (
    <>
      {stale !== undefined && (
        <Callout
          tone="gold"
          icon={TriangleAlert}
          action={
            <button
              type="button"
              className={linkClass}
              onClick={() => {
                setStamp(stale);
                setStale(undefined);
                onReload();
              }}
            >
              Reload
            </button>
          }
        >
          The game wrote slot {slot} since it was read. Reload to put your
          changes on the new file.
        </Callout>
      )}
      <UnsavedBar
        message={`${count(n, "change")} to slot ${slot}`}
        error={save.error}
        busy={save.busy}
        saveLabel="Save"
        why={running ? `${CLOSE_GAME}: it writes over its saves` : undefined}
        onDiscard={discard}
        onSave={write}
      />
    </>
  );
}
