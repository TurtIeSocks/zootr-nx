import { Gamepad2, Play } from "lucide-react";
import { WhyButton } from "@/components/why-button";
import { play } from "@/dev/requests";
import { useDev } from "@/stores/dev";

/** What the body shows while the game can't be read: how to start it. */
export function NotRunning() {
  const [target] = useDev.usePath("target");
  const [running] = useDev.usePath("activity.running");

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
      <Gamepad2 aria-hidden className="size-7 text-ink-muted" />
      <h2 className="font-semibold text-[15px]">
        Start the game to use the dev panel
      </h2>
      <p className="text-[13px] text-ink-muted">
        Warps, gives and values wait until the game can be read. The tabs above
        still show their lists.
      </p>
      {target && (
        <WhyButton
          variant="secondary"
          className="mt-1"
          why={running && `Wait for ${running.what} to finish`}
          onClick={() => void play(target)}
        >
          <Play />
          Play {target}
        </WhyButton>
      )}
    </div>
  );
}
