import { ExternalLink, Eye, Info, LoaderCircle, X } from "lucide-react";
import type { ReactNode } from "react";
import { commands, type IconSize } from "@/bindings";
import { Callout, InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { LoadGate } from "@/components/load-gate";
import { linkClass } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { ItemsGrid } from "@/items/items-grid";
import { move, TITLES } from "@/items/layout";
import { call } from "@/lib/api";
import { useRequestState } from "@/lib/request";
import { Page } from "@/shell/page-header";
import {
  type Options,
  setOptions,
  setSections,
  useItems,
} from "@/stores/items";
import { openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** The tile sizes, as the size picker's options. */
const SIZES: { id: IconSize; label: string }[] = (
  ["Small", "Medium", "Large"] as const
).map((s) => ({ id: s, label: s }));

/** The Items tab: the grid, its options and the Pop Out button. */
export function ItemsView() {
  const [loaded] = useItems.usePath("loaded");
  const [error] = useItems.usePath("error");
  return (
    <LoadGate loaded={loaded} error={error}>
      <Page wide className="min-h-0 flex-1">
        <OwnIconsBanner />
        <div className="flex min-h-0 flex-1 gap-5">
          {/* Room above the top sections for their toolbars. */}
          <div className="min-w-0 flex-1 overflow-auto rounded-lg border border-line bg-surface p-4 pt-12 shadow-xs">
            <ItemsGrid edit />
          </div>
          <aside className="flex w-64 shrink-0 flex-col gap-5 overflow-auto rounded-lg border border-line bg-surface p-4 shadow-xs">
            <PopOutButton />
            <InlineError error={error} />
            <Building />
            <Setting title="Size">
              <Size />
            </Setting>
            <Background />
            <Setting title="Layout">
              <p className="text-[13px] text-ink-muted leading-[18px]">
                Hover a section to move, resize or hide it.
              </p>
              <Hidden />
            </Setting>
          </aside>
        </div>
      </Page>
    </LoadGate>
  );
}

/** Says the tracker shows zootr-nx's own icons, and where to turn on the
 * game's. Hidden once dismissed, which `ui.json` keeps. */
function OwnIconsBanner() {
  const own = useItems((s) => s.art === "fallback");
  const [dismissed, setDismissed] = useUi.usePath("items.bannerDismissed");
  if (!own || dismissed) return null;
  return (
    <Callout
      tone="info"
      icon={Info}
      action={
        <div className="flex items-center gap-3">
          <button
            type="button"
            className={linkClass}
            onClick={() => openSettings("settings")}
          >
            Open Settings
          </button>
          <TipButton
            label="Dismiss"
            tip="Dismiss"
            variant="ghost"
            size="icon-sm"
            onClick={() => setDismissed(true)}
          >
            <X />
          </TipButton>
        </div>
      }
    >
      Showing zootr-nx's own icons. To draw the game's icons from your game
      dump, turn on game assets in Settings.
    </Callout>
  );
}

/** Opens the tracker in its own window, saying why if it can't. */
function PopOutButton() {
  const popOut = useRequestState();
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          className="flex-1"
          disabled={popOut.busy}
          onClick={() => popOut.run(() => call(commands.itemsPopOut()))}
        >
          <ExternalLink />
          Pop Out
        </Button>
        <Help topic="items" />
      </div>
      <InlineError error={popOut.error} />
    </div>
  );
}

/** Says the icons are still being built from the dump. */
function Building() {
  const [art] = useItems.usePath("art");
  if (art !== "building") return null;
  return (
    <Callout icon={LoaderCircle} className="[&>svg]:animate-spin">
      Building icons from your game dump. This happens once per dump.
    </Callout>
  );
}

/** Saves `patch` over the current options. */
const set = (patch: Partial<Options>) =>
  setOptions({ ...useItems.getPath("options"), ...patch });

/** The tile size. */
function Size() {
  const [size] = useItems.usePath("options.size");
  return (
    <Segmented
      full
      aria-label="Size"
      options={SIZES}
      value={size}
      onValueChange={(size) => set({ size })}
    />
  );
}

/** The grid's background color, for OBS to key out. The tracker can't be
 * transparent, so the hint says to key it instead. */
function Background() {
  const [background] = useItems.usePath("options.background");
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="justify-between">
        Background
        <input
          type="color"
          value={background}
          onChange={(e) => set({ background: e.target.value })}
          className="h-7 w-10 cursor-pointer rounded-sm border border-line-strong bg-transparent"
        />
      </Label>
      <p className="text-[13px] text-ink-muted leading-[18px]">
        The tracker can't be transparent. For a stream, pick a color your
        streaming software keys out
      </p>
    </div>
  );
}

/** A Show button for each hidden section. */
function Hidden() {
  const [hidden] = useItems.usePath("hidden");
  if (hidden.length === 0) return null;
  return (
    <div className="mt-1 flex flex-wrap gap-1.5">
      {hidden.map((s) => (
        <Button
          key={s}
          size="sm"
          variant="secondary"
          className="rounded-full"
          aria-label={`Show ${TITLES[s]}`}
          onClick={() => {
            const next = move(
              useItems.getPath("options.sections"),
              s,
              "show",
              0,
            );
            if (next) setSections(next);
          }}
        >
          <Eye />
          {TITLES[s]}
        </Button>
      ))}
    </div>
  );
}

/** Props for {@link Setting}. */
interface SettingProps {
  title: string;
  children: ReactNode;
}

/** A titled group of the side panel. */
function Setting({ title, children }: SettingProps) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="type-overline mb-2">{title}</legend>
      {children}
    </fieldset>
  );
}
