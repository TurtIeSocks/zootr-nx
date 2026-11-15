import { RotateCcw } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import type { SettingDef } from "@/bindings";
import { RequestError } from "@/components/request-error";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { defaults, isChanged, isOn } from "@/lib/settings";
import { Checklist } from "@/settings/checklist";
import { lockedBy, untested } from "@/settings/setting";
import { LockedBy, SettingRow } from "@/settings/setting-row";
import { SettingsSection } from "@/settings/settings-section";
import { PageHeader } from "@/shell/page-header";
import { changeSettings, useConfig } from "@/stores/config";

/** Props for {@link GroupPage}. */
export interface GroupPageProps {
  /** The schema group to show. */
  group: string;
}

/** One schema group, top to bottom: its settings in a card, then each
 * subgroup's toggles as a checklist. Call it inside a request, which the
 * header's reset runs in. */
export function GroupPage({ group }: GroupPageProps) {
  const [schema] = useConfig.usePath("schema");
  const defs = schema.filter((d) => d.group === group);
  const rows = defs.filter((d) => !listed(d));
  const subgroups = [
    ...new Set(defs.flatMap((d) => (listed(d) ? [d.subgroup ?? ""] : []))),
  ];
  return (
    <>
      <PageHeader title={group} actions={<ResetGroup defs={defs} />} />
      {rows.length > 0 && (
        <SettingsSection>
          {rows.map((d) => (
            <SettingRow key={d.id} def={d} />
          ))}
        </SettingsSection>
      )}
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

/** Puts every changed setting of a group back to its default. Nothing while
 * none is changed. */
function ResetGroup({ defs }: DefsProps) {
  const { run } = useRequest();
  const changed = useConfig(
    (s) => defs.filter((d) => isChanged(d, s.config.settings)).length,
  );
  if (changed === 0) return null;
  return (
    <Button
      variant="ghost"
      onClick={() => run(() => changeSettings(defaults(defs)))}
    >
      <RotateCcw />
      Reset {changed} change{changed === 1 ? "" : "s"}
    </Button>
  );
}

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
          tags: isUntested && <Badge variant="gold">Untested</Badge>,
          on: on[i] ?? false,
          locked: locks[i] !== undefined,
        };
      })}
      onChange={(changes) => run(() => changeSettings(changes))}
    />
  );
}
