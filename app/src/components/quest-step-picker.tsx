import type { ReactNode } from "react";
import type { Info } from "@/bindings";
import { NameField } from "@/components/name-field";
import { ValueInput } from "@/components/value-input";
import { type QuestStep, type Quests, stepOf, stepsOf } from "@/lib/quest-step";

/** Pinned, so a long step name can't squeeze the quest field shut. */
const STEP_WIDTH = "w-40 shrink-0";

/** Props for {@link QuestStepPicker}. */
interface QuestStepPickerProps {
  quests: Quests;
  value: QuestStep;
  onChange: (value: QuestStep) => void;
  label?: string;
  stepLabel?: string;
  id?: string;
  placeholder?: string;
  onEnter?: () => void;
  /** What acts on the pick, after the step: a button. */
  children?: ReactNode;
  /** Quests suggested first: the starred ones. */
  starred?: string[];
  /** Right after the quest field: its star. */
  star?: ReactNode;
}

/** A quest, suggested from the game's quests, and a choice of its steps
 * once it names one. */
export function QuestStepPicker({
  quests,
  value,
  onChange,
  label = "Quest",
  stepLabel = "Step",
  id,
  placeholder,
  onEnter,
  children,
  starred,
  star,
}: QuestStepPickerProps) {
  const steps = stepsOf(quests, value.challenge.trim()) ?? [];
  const info: Info = {
    kind: "Enum",
    input: "choice",
    choices: steps,
    default: null,
  };
  return (
    <div className="flex gap-1.5">
      <NameField
        id={id}
        label={label}
        placeholder={placeholder}
        names={Object.keys(quests)}
        value={value.challenge}
        onChange={(challenge) => onChange({ ...value, challenge })}
        onEnter={onEnter}
        first={starred}
      />
      {star}
      {steps.length > 0 && (
        <ValueInput
          label={stepLabel}
          info={info}
          value={stepOf(quests, value)}
          onChange={(step) => onChange({ ...value, step })}
          className={STEP_WIDTH}
        />
      )}
      {children}
    </div>
  );
}
