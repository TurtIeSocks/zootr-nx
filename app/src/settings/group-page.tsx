import { RotateCcw, ToggleLeft, ToggleRight } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import type { SettingDef } from "@/bindings";
import { RequestError } from "@/components/request-error";
import { WhyButton } from "@/components/why-button";
import { GROUP_HELP } from "@/help/topics";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import {
  defaults,
  isChanged,
  isOn,
  type Patch,
  setToggles,
} from "@/lib/settings";
import { Checklist } from "@/settings/checklist";
import { UntestedBadge } from "@/settings/row";
import { lockedBy, untested } from "@/settings/setting";
import { LockedBy, SettingRow } from "@/settings/setting-row";
import { SettingsSection } from "@/settings/settings-section";
import { StartInventory } from "@/settings/start-inventory";
import { PageHeader } from "@/shell/page-header";
import { changeSettings, useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

/** Props for {@link GroupPage}. */
export interface GroupPageProps {
  /** The schema group to show. */
  group: string;
}

/** One schema group, top to bottom: its settings in a card, a starting
 * inventory's grid under them, then each subgroup's toggles as a
 * checklist. Call it inside a request, which the
 * header's reset runs in. */
export function GroupPage({ group }: GroupPageProps) {
  const [schema] = useConfig.usePath("schema");
  const defs = schema.filter((d) => d.group === group);
  const rows = defs.filter((d) => !listed(d));
  const grids = rows.filter((d) => d.kind.type === "inventory");
  const subgroups = [
    ...new Set(defs.flatMap((d) => (listed(d) ? [d.subgroup ?? ""] : []))),
  ];
  return (
    <>
      <PageHeader
        title={group}
        help={GROUP_HELP[group]}
        actions={<GroupActions defs={defs} />}
      />
      {rows.length > 0 && (
        <SettingsSection>
          {rows.map((d) => (
            <SettingRow key={d.id} def={d} />
          ))}
        </SettingsSection>
      )}
      {grids.map((d) => (
        <StartInventory key={d.id} def={d} />
      ))}
      {subgroups.map((g) => (
        <Subgroup
          key={g}
          title={g}
          defs={defs.filter((d) => listed(d) && d.subgroup === g)}
        />
      ))}
    </>
  );
}

/** Whether a setting is listed in its subgroup's checklist, not a row. */
const listed = (d: SettingDef) => d.subgroup && d.kind.type === "toggle";

/** Props for {@link ResetGroup} and {@link Subgroup}. */
interface DefsProps {
  defs: SettingDef[];
}

/** The page's actions: Enable all and Disable all on a page with toggles,
 * and Reset to defaults. Each says why when it would change nothing. */
function GroupActions({ defs }: DefsProps) {
  const { run } = useRequest();
  const [schema] = useConfig.usePath("schema");
  const [settings] = useConfig.usePath("config.settings");
  const set = (patch: Patch) => () => run(() => changeSettings(patch));
  const enable = setToggles(defs, schema, settings, true);
  const disable = setToggles(defs, schema, settings, false);
  const changed = defs.some((d) => isChanged(d, settings));
  const toggles = defs.some((d) => d.kind.type === "toggle");
  return (
    <>
      {toggles && (
        <>
          <WhyButton
            variant="ghost"
            why={empty(enable) && "Every toggle here is already on"}
            onClick={set(enable)}
          >
            <ToggleRight />
            Enable all
          </WhyButton>
          <WhyButton
            variant="ghost"
            why={empty(disable) && "Every toggle here is already off"}
            onClick={set(disable)}
          >
            <ToggleLeft />
            Disable all
          </WhyButton>
        </>
      )}
      <WhyButton
        variant="ghost"
        why={!changed && "Already the defaults"}
        onClick={set(defaults(defs))}
      >
        <RotateCcw />
        Reset to defaults
      </WhyButton>
    </>
  );
}

/** Whether a patch changes nothing. */
const empty = (patch: Patch) => Object.keys(patch).length === 0;

/** Props for {@link Subgroup}. */
interface SubgroupProps extends DefsProps {
  title: string;
}

/** A subgroup's toggles as one checklist, whose changes save one at a time.
 * A description every item shares shows once, under the title. */
function Subgroup(props: SubgroupProps) {
  return (
    <RequestProvider>
      <SubgroupList {...props} />
    </RequestProvider>
  );
}

/** The checklist of a subgroup, inside its request. */
function SubgroupList({ title, defs }: SubgroupProps) {
  const { run } = useRequest();
  const on = useConfig(
    useShallow((s) => defs.map((d) => isOn(d, s.config.settings, s.schema))),
  );
  const locks = useConfig(
    useShallow((s) =>
      defs.map((d) => lockedBy(d, s.schema, s.config.settings)),
    ),
  );
  const drifted = useSeeds(
    useShallow((s) => defs.map((d) => s.drift.some((c) => c.def.id === d.id))),
  );
  const shared = new Set(defs.map((d) => d.description)).size === 1;
  const parents = [...new Set(locks.flatMap((p) => p ?? []))];
  return (
    <Checklist
      title={title}
      description={shared ? defs[0]?.description : undefined}
      note={
        <>
          {parents.map((p) => (
            <LockedBy key={p.id} parent={p} />
          ))}
          <RequestError className="mt-1.5" />
        </>
      }
      items={defs.map((d, i) => {
        const [isUntested, description] = untested(d.description);
        return {
          id: d.id,
          label: d.label,
          description: shared ? undefined : description || undefined,
          tags: isUntested && <UntestedBadge />,
          on: on[i] ?? false,
          changed: drifted[i],
          locked: locks[i] !== undefined,
        };
      })}
      onChange={(changes) => run(() => changeSettings(changes))}
    />
  );
}
