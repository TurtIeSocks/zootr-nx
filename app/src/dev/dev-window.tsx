import { Pin, PinOff } from "lucide-react";
import { useEffect } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Give } from "@/dev/give";
import { InfinitySwitches } from "@/dev/infinity-switches";
import { DevSection } from "@/dev/section";
import { SetValue } from "@/dev/set-value";
import { Teleport } from "@/dev/teleport";
import { TestStarts } from "@/dev/test-starts";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { init, pin, useDev } from "@/stores/dev";

/** The dev panel's window: what the game is doing, and requests for it. It
 * floats over the game until unpinned. */
export function DevWindow() {
  useEffect(() => {
    void init();
  }, []);

  return (
    <div className="flex h-full flex-col overflow-auto bg-surface text-ink">
      <header className="flex items-start justify-between gap-2 px-3 py-2.5">
        <Status />
        <PinButton />
      </header>
      <DevError />
      <Teleport />
      <Age />
      <InfinitySwitches />
      <Give />
      <SetValue />
      <TestStarts />
    </div>
  );
}

/** Where Link is and at which age, or why the game can't be read. */
function Status() {
  const [{ state, error }] = useDev.usePath("game");
  if (!state) {
    return <p className="text-ink-faint text-sm">{error ?? "Looking…"}</p>;
  }
  const busy = state.loading ? "loading" : state.event ? "event playing" : "";
  return (
    <p className="text-sm">
      <span className="font-medium">{state.map}</span>
      <span className="text-ink-muted">
        {" "}
        as {state.child ? "child" : "adult"}
      </span>
      {busy && <span className="text-gold-ink"> · {busy}</span>}
    </p>
  );
}

/** Swaps Link's age. */
function Age() {
  return (
    <DevSection title="Age">
      <AgeButton />
    </DevSection>
  );
}

/** Switches to the age Link isn't, once the game says which he is. */
function AgeButton() {
  const [child] = useDev.usePath("game.state.child");
  const [ready] = useDev.usePath("ready");
  const request = useRequest();
  return (
    <Button
      variant="secondary"
      disabled={!ready || request.busy || child === undefined}
      onClick={() => request.run(() => call(commands.devAge()))}
    >
      {child === false ? "Switch to child" : "Switch to adult"}
    </Button>
  );
}

/** Why the dev options didn't load. */
function DevError() {
  const [error] = useDev.usePath("error");

  if (!error) return null;
  return <p className="px-3 pb-2 text-danger-ink text-xs">{error}</p>;
}

/** Keeps the window over the game, or lets it drop behind. */
function PinButton() {
  const [pinned] = useDev.usePath("pinned");
  return (
    <Button
      size="icon-sm"
      variant={pinned ? "selected" : "ghost"}
      aria-label={pinned ? "Unpin from the top" : "Keep on top"}
      aria-pressed={pinned}
      onClick={pin}
    >
      {pinned ? <Pin /> : <PinOff />}
    </Button>
  );
}
