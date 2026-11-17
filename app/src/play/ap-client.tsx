import { Plug, Unplug } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type Summary } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { call, message } from "@/lib/api";
import { init, useAp } from "@/stores/ap";
import { useUi } from "@/stores/ui";

const STATUS = {
  offline: "Offline",
  connecting: "Connecting",
  connected: "Connected",
  refused: "Refused",
} as const;

/** Connects the running game to an Archipelago room as the installed
 * seed's slot. The server and password are kept per seed. */
export function ApClient({ entry }: { entry: Summary }) {
  const { status, log } = useAp();
  const [rooms, setRooms] = useUi.usePath("ap.rooms");
  const room = rooms[entry.id] ?? { server: "", password: "" };
  const [error, setError] = useState<string>();
  const running = status === "connecting" || status === "connected";

  useEffect(() => {
    void init();
  }, []);

  const edit = (part: Partial<typeof room>) =>
    setRooms({ ...rooms, [entry.id]: { ...room, ...part } });

  const connect = async () => {
    setError(undefined);
    try {
      await call(commands.apConnect(room.server, room.password || null));
    } catch (e) {
      setError(message(e));
    }
  };

  return (
    <div className="flex flex-col gap-3 border-line border-t pt-4">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="type-overline mr-2">Archipelago</h3>
        <Badge>{entry.ap_slot}</Badge>
        <Badge>{STATUS[status]}</Badge>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          className="h-9 w-64"
          placeholder="archipelago.gg:38281"
          aria-label="Server"
          value={room.server}
          disabled={running}
          onChange={(e) => edit({ server: e.target.value })}
        />
        <Input
          className="h-9 w-44"
          type="password"
          placeholder="Password"
          aria-label="Password"
          value={room.password}
          disabled={running}
          onChange={(e) => edit({ password: e.target.value })}
        />
        {running ? (
          <Button
            variant="secondary"
            onClick={() => void commands.apDisconnect()}
          >
            <Unplug />
            Disconnect
          </Button>
        ) : (
          <Button disabled={!room.server} onClick={() => void connect()}>
            <Plug />
            Connect
          </Button>
        )}
      </div>
      {error && <p className="text-[13px] text-danger-ink">{error}</p>}
      {log.length > 0 && (
        <ol className="max-h-40 overflow-y-auto font-mono text-[12px] text-ink-muted">
          {log.map((line, i) => (
            // The log only grows at its end, so an index keys it.
            // biome-ignore lint/suspicious/noArrayIndexKey: append-only
            <li key={i}>{line}</li>
          ))}
        </ol>
      )}
    </div>
  );
}
