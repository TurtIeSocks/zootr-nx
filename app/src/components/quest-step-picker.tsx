import type { ReactNode } from "react";
import type { Info } from "@/bindings";
import { NameField } from "@/components/name-field";
import { ValueInput } from "@/components/value-input";
import { type QuestStep, type Quests, stepOf } from "@/lib/quest-step";

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
  /** Classes for the step's choice. */
  stepClassName?: string;
  /** What acts on the pick, after the step: a button. */
  children?: ReactNode;
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
  stepClassName,
  children,
}: QuestStepPickerProps) {
  const steps = quests[value.challenge.trim()] ?? [];
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
      />
      {steps.length > 0 && (
        <ValueInput
          label={stepLabel}
          info={info}
          value={stepOf(quests, value)}
          onChange={(step) => onChange({ ...value, step })}
          className={stepClassName}
        />
      )}
      {children}
    </div>
  );
}
