import { open as pickFile, save as pickSave } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Download, Eye, EyeOff, FileUp, FolderOpen } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { commands, type Summary } from "@/bindings";
import { InlineError } from "@/components/callout";
import { CardHeader } from "@/components/card";
import { linkClass } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { call, message } from "@/lib/api";
import { useCopied } from "@/lib/use-copied";
import { ApLog } from "@/play/ap-log";
import { GenerateSteps } from "@/play/generate-steps";
import { DeathLinkBadge, RoomStatus } from "@/play/room-status";
import { useApznxDrop } from "@/play/use-apznx-drop";
import { useAp } from "@/stores/ap";
import { generateAp, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { AP_GUIDE_URL, apworldUrl } from "@/update/release";

/** The Archipelago folder: open a multiworld slot, connect the game to its
 * room, and watch what the client does. */
export function ArchipelagoPanel() {
  const installed = useSeeds((s) => s.history.installed);
  const slot = installed?.ap_slot ? installed : null;
  return (
    <>
      <Slot slot={slot?.ap_slot ?? null} />
      <section className="border-line border-t">
        <CardHeader
          title="Room"
          help="ap-room"
          description={
            slot
              ? `Where ${slot.ap_slot} plays. Kept with this seed.`
              : undefined
          }
          action={slot?.death_link && <DeathLinkBadge />}
        />
        <div className="flex flex-col gap-3 px-4 pb-4">
          {slot ? (
            <Room entry={slot} />
          ) : (
            <p className="text-[13px] text-ink-muted">
              Install an Archipelago slot to connect to its room
            </p>
          )}
        </div>
      </section>
      <ApLog />
    </>
  );
}

/** Opens a slot's `.apznx`, dropped or browsed for, with its steps below. */
function Slot({ slot }: { slot: string | null }) {
  const [busy] = useSeeds.usePath("busy");
  const [blocked] = useSeeds.usePath("blocked");
  const why = blocked || (busy ? "A seed is already being made" : "");
  // A drop that was turned away, whose reason shows while it still holds.
  const [refused, setRefused] = useState(false);
  if (refused && !why) setRefused(false);
  const open = useCallback(
    (path: string) => {
      setRefused(!!why);
      if (!why) void generateAp(path);
    },
    [why],
  );
  const over = useApznxDrop(open);
  const browse = async () => {
    const file = await pickFile({
      filters: [{ name: "Archipelago seed", extensions: ["apznx"] }],
    });
    if (file) open(file);
  };
  const apworld = useApworld();
  return (
    <section>
      <CardHeader
        title="Slot"
        help="ap-slot"
        action={<ForHosts apworld={apworld} />}
      />
      <div className="flex flex-col gap-3 px-4 pb-4">
        <div
          data-over={over}
          className="flex items-center gap-4 rounded-md border-[1.5px] border-control-border border-dashed bg-surface-sunken p-4 transition-colors duration-(--duration-fast) data-[over=true]:border-focus data-[over=true]:border-solid data-[over=true]:bg-primary-soft"
        >
          <span className="grid size-10 place-items-center rounded-md bg-surface text-ink-muted shadow-[var(--shadow-xs),0_0_0_1px_var(--line)] [&_svg]:size-5">
            <FileUp />
          </span>
          <div className="min-w-0 flex-1">
            <p className="font-medium text-sm">Drop a slot's .apznx here</p>
            <p className="mt-0.5 text-[12.5px] text-ink-muted leading-[17px]">
              It brings its own settings and replaces the installed seed.
              {slot && ` Installed now: ${slot}.`}
            </p>
          </div>
          <TipButton
            label="Browse…"
            tip={blocked || "Pick your slot's .apznx"}
            variant="secondary"
            size="default"
            disabled={busy || !!blocked}
            onClick={browse}
          >
            <FolderOpen />
            Browse…
          </TipButton>
        </div>
        {refused && why && (
          <InlineError error={`The file wasn't opened. ${why}`} />
        )}
        <GenerateSteps ap />
        <p className="text-[12.5px] text-ink-muted leading-[17px]">
          Your host sends your .apznx and the room's server.{" "}
          <button
            type="button"
            className={linkClass}
            onClick={() => void openUrl(AP_GUIDE_URL)}
          >
            Setup Guide
          </button>
        </p>
        <InlineError error={apworld.error} />
      </div>
    </section>
  );
}

/** The host's apworld. A release build saves its own copy, so it always
 * matches the app. A build without one links the release's. */
function useApworld() {
  const [version] = useUpdate.usePath("current");
  const [bundled, setBundled] = useState(false);
  useEffect(() => {
    commands
      .apworldBundled()
      .then(setBundled, (e: unknown) =>
        console.debug("Couldn't ask for the apworld", e),
      );
  }, []);
  const { copied: saved, flash } = useCopied();
  const [error, setError] = useState<string>();
  const name = version ? `znx.apworld ${version}` : "znx.apworld";

  const get = async () => {
    if (!bundled) {
      void openUrl(apworldUrl(version));
      return;
    }
    const path = await pickSave({
      defaultPath: "znx.apworld",
      filters: [{ name: "Archipelago world", extensions: ["apworld"] }],
    });
    if (!path) return;
    try {
      await call(commands.saveApworld(path));
      setError(undefined);
      flash("apworld");
    } catch (e) {
      setError(message(e));
    }
  };
  return { bundled, saved: !!saved, error, name, get };
}

/** The Slot header's corner: the apworld, which only a host needs. */
function ForHosts({ apworld }: { apworld: ReturnType<typeof useApworld> }) {
  const { bundled, saved, name, get } = apworld;
  return (
    <span className="flex items-center gap-2 text-ink-faint text-xs">
      For hosts:
      <TipButton
        label={saved ? "Saved" : "Save apworld"}
        tip={
          bundled
            ? `Save ${name}, built with this app, for Archipelago`
            : `Open ${name} on its release page`
        }
        variant="secondary"
        size="sm"
        onClick={() => void get()}
      >
        {saved ? <Check /> : <Download />}
        {saved ? "Saved" : "Save apworld"}
      </TipButton>
    </span>
  );
}

/** The room's server and password, kept with the seed, and its status.
 * Both lock while the client is connecting or connected. */
function Room({ entry }: { entry: Summary }) {
  const status = useAp((s) => s.status);
  const error = useAp((s) => s.error);
  const [rooms, setRooms] = useUi.usePath("ap.rooms");
  const room = rooms[entry.id] ?? { server: "", password: "" };
  const [shown, setShown] = useState(false);
  const server = useRef<HTMLInputElement>(null);
  const id = useId();
  const locked = status === "connecting" || status === "connected";
  const edit = (part: Partial<typeof room>) =>
    setRooms({ ...rooms, [entry.id]: { ...room, ...part } });
  return (
    <>
      <div className="grid grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)] gap-3">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor={`${id}-server`}>Server</Label>
          <Input
            ref={server}
            id={`${id}-server`}
            className="h-8 font-mono"
            placeholder="archipelago.gg:38281"
            value={room.server}
            disabled={locked}
            onChange={(e) => edit({ server: e.target.value })}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor={`${id}-password`}>Password</Label>
          <div className="relative">
            <Input
              id={`${id}-password`}
              className="h-8 pr-9"
              type={shown ? "text" : "password"}
              placeholder="None"
              value={room.password}
              disabled={locked}
              onChange={(e) => edit({ password: e.target.value })}
            />
            <Button
              variant="ghost"
              size="icon-xs"
              className="-translate-y-1/2 absolute top-1/2 right-1"
              aria-label={shown ? "Hide password" : "Show password"}
              aria-pressed={shown}
              onClick={() => setShown(!shown)}
            >
              {shown ? <EyeOff /> : <Eye />}
            </Button>
          </div>
        </div>
      </div>
      <RoomStatus entry={entry} onSetUp={() => server.current?.focus()} />
      <InlineError error={error ?? undefined} />
    </>
  );
}
