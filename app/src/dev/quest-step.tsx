import { commands, type Info } from "@/bindings";
import { NameField } from "@/components/name-field";
import { Button } from "@/components/ui/button";
import { ValueInput } from "@/components/value-input";
import { DevSection } from "@/dev/section";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Moves a quest to a step through the game's quest system, as its own
 * scenes do. Set only writes the save's copy, which the quest ignores. */
export function QuestStep() {
  return (
    <DevSection title="Quest step">
      <div className="flex gap-1.5">
        <QuestSelect />
        <StepSelect />
        <StepButton />
      </div>
    </DevSection>
  );
}

/** The picked quest's steps, and the step to send: the picked one, else
 * its first. */
function useStep() {
  const [quests] = useDev.usePath("options.quests");
  const [{ challenge, step }] = useUi.usePath("dev.quest");
  const steps = quests[challenge.trim()] ?? [];
  return { steps, step: steps.includes(step) ? step : (steps[0] ?? "") };
}

/** The quest, suggested from the game's quests. */
function QuestSelect() {
  const [quests] = useDev.usePath("options.quests");
  const [challenge, setChallenge] = useUi.usePath("dev.quest.challenge");

  return (
    <NameField
      label="Quest"
      names={Object.keys(quests)}
      value={challenge}
      onChange={setChallenge}
    />
  );
}

/** The step, from the picked quest's steps in order. */
function StepSelect() {
  const { steps, step } = useStep();
  const [, setStep] = useUi.usePath("dev.quest.step");
  if (!steps.length) return null;

  const info: Info = {
    kind: "Enum",
    input: "choice",
    choices: steps,
    default: null,
  };
  return (
    <ValueInput label="Step" info={info} value={step} onChange={setStep} />
  );
}

/** Sends the step, once a quest with steps is picked and the game can be
 * read. */
function StepButton() {
  const [ready] = useDev.usePath("ready");
  const [challenge] = useUi.usePath("dev.quest.challenge");
  const { step } = useStep();
  const request = useRequest();

  return (
    <Button
      variant="secondary"
      disabled={!ready || request.busy || !step}
      onClick={() =>
        request.run(() => call(commands.devStep(challenge.trim(), step)))
      }
    >
      Move
    </Button>
  );
}
