/** Each quest the game has and its steps, in order. */
export type Quests = Record<string, string[]>;

/** A quest and one of its steps. */
export interface QuestStep {
  challenge: string;
  step: string;
}

/** The steps of the quest named `name`, if the game has it. Own keys only,
 * so a name like `constructor` isn't read off the prototype. */
export function stepsOf(quests: Quests, name: string): string[] | undefined {
  return Object.hasOwn(quests, name) ? quests[name] : undefined;
}

/** The step to use for `pick`: the picked one if its quest has it, else
 * the quest's first. Empty for a quest the game doesn't have. */
export function stepOf(quests: Quests, pick: QuestStep): string {
  const steps = stepsOf(quests, pick.challenge.trim()) ?? [];
  return steps.includes(pick.step) ? pick.step : (steps[0] ?? "");
}

/** What a quest's saved step is named after: `Step_` and the quest. */
export const SAVED = "Step_";
