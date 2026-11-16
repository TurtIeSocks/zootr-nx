import { commands } from "@/bindings";
import { QuestStepPicker } from "@/components/quest-step-picker";
import { Button } from "@/components/ui/button";
import { DevSection } from "@/dev/section";
import { call } from "@/lib/api";
import { stepOf } from "@/lib/quest-step";
import { useRequest } from "@/lib/request";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Moves a quest to a step through the game's quest system, as its own
 * scenes do. Set only writes the save's copy, which the quest ignores. */
export function QuestStep() {
  const [quests] = useDev.usePath("options.quests");
  const [pick, setPick] = useUi.usePath("dev.quest");

  return (
    <DevSection title="Quest step">
      <QuestStepPicker quests={quests} value={pick} onChange={setPick}>
        <MoveButton />
      </QuestStepPicker>
    </DevSection>
  );
}

/** Sends the step, once a quest with steps is picked and the game can be
 * read. */
function MoveButton() {
  const [quests] = useDev.usePath("options.quests");
  const [pick] = useUi.usePath("dev.quest");
  const [ready] = useDev.usePath("ready");
  const request = useRequest();
  const step = stepOf(quests, pick);

  return (
    <Button
      variant="secondary"
      disabled={!ready || request.busy || !step}
      onClick={() =>
        request.run(() => call(commands.devStep(pick.challenge.trim(), step)))
      }
    >
      Move
    </Button>
  );
}
