import { Gift, Minus, Plus, Signpost } from "lucide-react";
import { useEffect, useState } from "react";
import { commands } from "@/bindings";
import { NameField } from "@/components/name-field";
import { QuestStepPicker } from "@/components/quest-step-picker";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DevCard } from "@/dev/dev-card";
import { give, giveLabel } from "@/dev/requests";
import { call, message } from "@/lib/api";
import { SAVED, stepOf } from "@/lib/quest-step";
import { send, useCanSend, useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** The most Give gives at once. */
const MAX_COUNT = 99;

/** Gives an item, and moves a quest to a step. */
export function GiveTab() {
  return (
    <div className="flex flex-col gap-2.5">
      <GiveCard />
      <QuestStepCard />
    </div>
  );
}

/** Gives Link an item, as many as the count says, and repeats a recent
 * give in one click. */
function GiveCard() {
  return (
    <DevCard title="Give" icon={Gift}>
      <div className="flex gap-1.5">
        <GiveSelect />
        <GiveCount />
        <GiveButton />
      </div>
      <RecentGives />
    </DevCard>
  );
}

/** The item to give, suggested from the game's item names. */
function GiveSelect() {
  const [items] = useDev.usePath("options.items");
  const [item, setItem] = useUi.usePath("dev.give.item");

  return (
    <NameField label="Item" names={items} value={item} onChange={setItem} />
  );
}

/** `n` kept within what Give takes. */
const clamp = (n: number) =>
  Math.min(MAX_COUNT, Math.max(1, Math.round(n) || 1));

/** How many to give, 1 to 99, with a step down and up. */
function GiveCount() {
  const [count, setCount] = useUi.usePath("dev.give.count");

  return (
    <div className="flex h-8 shrink-0 items-center rounded-md border border-control-border bg-surface">
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label="One fewer"
        disabled={count <= 1}
        onClick={() => setCount(clamp(count - 1))}
      >
        <Minus />
      </Button>
      <Input
        aria-label="Count"
        type="number"
        min={1}
        max={MAX_COUNT}
        className="h-7 w-9 border-0 px-0 text-center shadow-none [appearance:textfield] focus-visible:ring-0 [&::-webkit-inner-spin-button]:appearance-none"
        value={count}
        onChange={(e) => setCount(clamp(Number(e.target.value)))}
      />
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label="One more"
        disabled={count >= MAX_COUNT}
        onClick={() => setCount(clamp(count + 1))}
      >
        <Plus />
      </Button>
    </div>
  );
}

/** Sends the give, once an item is named and the game can be read. */
function GiveButton() {
  const can = useCanSend();
  const [item] = useUi.usePath("dev.give.item");
  const [count] = useUi.usePath("dev.give.count");

  return (
    <Button
      disabled={!can || !item.trim()}
      onClick={() => void give(item.trim(), count)}
    >
      Give
    </Button>
  );
}

/** The last gives, each given again with a click. */
function RecentGives() {
  const [gives] = useUi.usePath("dev.recent.gives");
  const can = useCanSend();
  if (gives.length === 0) return null;

  return (
    <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
      <span className="text-ink-faint text-xs">Recent</span>
      {gives.map((g) => (
        <Button
          key={giveLabel(g)}
          size="sm"
          variant="secondary"
          aria-label={`Give ${giveLabel(g)} again`}
          disabled={!can}
          onClick={() => void give(g.item, g.count)}
        >
          {giveLabel(g)}
        </Button>
      ))}
    </div>
  );
}

/** Moves a quest to a step through the game's quest system, as its own
 * scenes do, and shows what its saved copy holds. Set only writes the
 * save's copy, which the quest ignores. */
function QuestStepCard() {
  const [quests] = useDev.usePath("options.quests");
  const [pick, setPick] = useUi.usePath("dev.quest");
  const challenge = quests[pick.challenge.trim()] ? pick.challenge.trim() : "";
  const [round, setRound] = useState(0);

  return (
    <DevCard
      title="Quest step"
      icon={Signpost}
      meta="through the game's quest system"
    >
      <QuestStepPicker quests={quests} value={pick} onChange={setPick}>
        <MoveButton onMoved={() => setRound((r) => r + 1)} />
      </QuestStepPicker>
      {challenge && <SavedCopy challenge={challenge} round={round} />}
    </DevCard>
  );
}

/** Sends the step, once a quest with steps is picked and the game can be
 * read. */
function MoveButton({ onMoved }: { onMoved: () => void }) {
  const [quests] = useDev.usePath("options.quests");
  const [pick] = useUi.usePath("dev.quest");
  const can = useCanSend();
  const step = stepOf(quests, pick);
  const quest = pick.challenge.trim();

  return (
    <Button
      variant="secondary"
      disabled={!can || !step}
      onClick={async () => {
        await send("step", `move ${quest} to ${step}`, () =>
          call(commands.devStep(quest, step)),
        );
        onMoved();
      }}
    >
      Move
    </Button>
  );
}

/** What the quest's saved `Step_` copy holds, read when the quest is
 * picked and after each Move. */
function SavedCopy({ challenge, round }: { challenge: string; round: number }) {
  const name = `${SAVED}${challenge}`;
  const [held, setHeld] = useState<{ name: string; text: string }>();

  useEffect(() => {
    // Read again after `round` moves on.
    void round;
    let live = true;
    call(commands.devValue(name))
      .then(
        (v) =>
          live &&
          setHeld({
            name,
            text: v.live ?? `${v.info.default ?? "—"} (default)`,
          }),
      )
      .catch((e) => live && setHeld({ name, text: message(e) }));
    return () => {
      live = false;
    };
  }, [name, round]);

  return (
    <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 text-[12.5px]">
      <dt className="text-ink-faint">Saved copy</dt>
      <dd className="type-mono truncate">
        {name} = {held?.name === name ? held.text : "…"}
      </dd>
    </dl>
  );
}
