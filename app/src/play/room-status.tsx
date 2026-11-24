import { Skull } from "lucide-react";
import type { ReactNode } from "react";
import type { Summary } from "@/bindings";
import { Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { reconnecting, refusal } from "@/play/ap-lines";
import { connect, disconnect, useAp } from "@/stores/ap";
import { useLive } from "@/stores/live";
import { useUi } from "@/stores/ui";

/** The room's dot: hollow while there's no connection. */
const DOT = {
  off: "shadow-[inset_0_0_0_1.5px_var(--ink-faint)]",
  wait: "animate-pulse bg-gold",
  on: "bg-primary",
  bad: "bg-danger",
} as const;

/** Props for {@link RoomStatus}. */
interface RoomStatusProps {
  /** The installed AP slot. */
  entry: Summary;
  /** Set up room, where the fields are. Without it, it opens the
   * Archipelago folder. */
  onSetUp?: () => void;
}

/** One line saying whether the game is talking to its Archipelago room,
 * with the one step that fits. The bold word names the state, so the dot
 * is never alone. */
export function RoomStatus({ entry, onSetUp }: RoomStatusProps) {
  const status = useAp((s) => s.status);
  const lines = useAp((s) => s.lines);
  const [rooms] = useUi.usePath("ap.rooms");
  const server = rooms[entry.id]?.server ?? "";
  const slot = entry.ap_slot ?? "";
  const addr = server && (
    <span className="font-mono text-[12.5px] text-ink-muted">{server}</span>
  );
  const go = () => void connect(entry.id);
  // The client reaches the room through the running game.
  const playing = useLive((s) => s.status.state !== "Waiting");

  if (status === "connected") {
    return (
      <Strip
        dot="on"
        action={<Act variant="ghost" label="Disconnect" onClick={disconnect} />}
      >
        <span>
          <b className="font-semibold">Connected</b> as {slot}
        </span>
        {addr}
        {entry.death_link && <DeathLinkBadge />}
      </Strip>
    );
  }
  if (status === "connecting") {
    return (
      <Strip
        dot="wait"
        action={<Act variant="ghost" label="Cancel" onClick={disconnect} />}
      >
        <b className="font-semibold">
          {reconnecting(lines) ? "Reconnecting" : "Connecting"}
        </b>
        {addr}
      </Strip>
    );
  }
  if (status === "refused") {
    return (
      <Strip dot="bad" action={<Act label="Try Again" onClick={go} />}>
        <span>
          <b className="font-semibold">Refused:</b> {refusal(lines, slot)}
        </span>
      </Strip>
    );
  }
  if (!server) {
    const setUp = onSetUp ?? (() => useUi.setPath("play.tab", "archipelago"));
    return (
      <Strip dot="off" action={<Act label="Set Up Room" onClick={setUp} />}>
        <b className="font-semibold">No room yet</b>
        <span className="text-ink-muted">
          Add the server in the Archipelago tab
        </span>
      </Strip>
    );
  }
  return (
    <Strip
      dot="off"
      action={
        <Act
          variant="default"
          label="Connect"
          onClick={go}
          disabled={!playing}
        />
      }
    >
      <b className="font-semibold">Not connected</b>
      {addr}
      {!playing && <span className="text-ink-muted">Start the game first</span>}
    </Strip>
  );
}

/** The DeathLink mark, for a slot that plays it. */
export function DeathLinkBadge() {
  return (
    <Tip focusable tip="Your deaths reach the others, and theirs reach you">
      <Badge variant="danger" shape="square">
        <Skull />
        DeathLink
      </Badge>
    </Tip>
  );
}

/** Props for {@link Strip}. */
interface StripProps {
  /** The room's state. A live connection's dot pings, and a refusal sits
   * on a danger ground. */
  dot: keyof typeof DOT;
  action: ReactNode;
  children: ReactNode;
}

/** The room line's sunken strip: dot, words, then its action. */
function Strip({ dot, action, children }: StripProps) {
  return (
    <div
      role="status"
      className={cn(
        "flex items-center gap-3 rounded-md py-2 pr-2 pl-3 text-[13px]",
        dot === "bad" ? "bg-danger-soft text-danger-ink" : "bg-surface-sunken",
      )}
    >
      <span className="relative flex size-[7px] shrink-0">
        {dot === "on" && (
          <span className="absolute inset-0 animate-ping rounded-full bg-primary opacity-60" />
        )}
        <span className={cn("relative size-[7px] rounded-full", DOT[dot])} />
      </span>
      <span className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
        {children}
      </span>
      {action}
    </div>
  );
}

/** Props for {@link Act}. */
interface ActProps {
  label: string;
  onClick: () => void;
  variant?: "default" | "ghost" | "secondary";
  disabled?: boolean;
}

/** The room line's one action. */
function Act({ label, onClick, variant = "secondary", disabled }: ActProps) {
  return (
    <Button size="sm" variant={variant} onClick={onClick} disabled={disabled}>
      {label}
    </Button>
  );
}
