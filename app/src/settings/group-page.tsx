import { useShallow } from "zustand/react/shallow";
import type { SettingDef } from "@/bindings";
import { RequestError } from "@/components/request-error";
import { divided } from "@/components/styles";
import { GROUP_HELP } from "@/help/topics";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { isOn, type Patch, setToggles, tally } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { AllNone, Checklist } from "@/settings/checklist";
import { Fold } from "@/settings/fold";
import { ResetButton } from "@/settings/reset-button";
import { ApBadge, UntestedBadge } from "@/settings/row";
import { apMark, controlId, lockedBy, untested } from "@/settings/setting";
import { LockedBy, SettingRow } from "@/settings/setting-row";
import { StartInventory } from "@/settings/start-inventory";
import { PageHeader } from "@/shell/page-header";
import { changeSettings, useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

/** Props for {@link GroupPage}. */
export interface GroupPageProps {
  /** The schema group to show. */
  group: string;
}

/** One schema group, top to bottom: a starting inventory's grid, then each
 * section of its settings, a checklist or a card of rows, in the order its
 * settings first reach them. Call it inside a request, which the header's
 * reset runs in. */
export function GroupPage({ group }: GroupPageProps) {
  const [schema] = useConfig.usePath("schema");
  const defs = schema.filter((d) => d.group === group);
  const grids = defs.filter((d) => d.kind.type === "inventory");
  const rest = defs.filter((d) => d.kind.type !== "inventory");
  const sections = [...new Set(rest.map((d) => d.section))];
  return (
    <>
      <PageHeader
        title={group}
        help={GROUP_HELP[group]}
        actions={<ResetButton defs={defs} />}
      />
      {grids.map((d) => (
        <StartInventory key={d.id} def={d} />
      ))}
      {sections.map((title) => {
        const inside = rest.filter((d) => d.section === title);
        const props = { id: `${group}/${title}`, defs: inside };
        return inside.every(listed) ? (
          <Subgroup key={title} title={title} {...props} />
        ) : (
          <Rows key={title} title={title} {...props} />
        );
      })}
    </>
  );
}

/** Whether a setting is listed in its section's checklist, not a row. */
const listed = (d: SettingDef) => d.list && d.kind.type === "toggle";

/** Props for the sections. */
interface DefsProps {
  defs: SettingDef[];
}

/** Whether a patch changes nothing. */
const empty = (patch: Patch) => Object.keys(patch).length === 0;

/** Props for {@link Rows} and {@link Subgroup}. */
interface SectionProps extends DefsProps {
  /** Keys whether it's open. */
  id: string;
  title: string;
}

/** A section's description and count, kept up to date. */
const useHeader = (defs: SettingDef[]) => ({
  description: defs[0]?.section_about,
  meta: useConfig((s) => tally(defs, s.config.settings, s.schema)),
});

/** A section's settings, one row each, with All and None while it holds
 * toggles. */
function Rows({ id, title, defs }: SectionProps) {
  const { run } = useRequest();
  const toggles = defs.some((d) => d.kind.type === "toggle");
  const [schema] = useConfig.usePath("schema");
  const [settings] = useConfig.usePath("config.settings");
  const patch = (on: boolean) => setToggles(defs, schema, settings, on);
  return (
    <Fold id={id} title={title} {...useHeader(defs)}>
      {toggles && (
        <AllNone
          of={title}
          would={(on) => !empty(patch(on))}
          onPick={(on) => run(() => changeSettings(patch(on)))}
        />
      )}
      <div className={cn("px-1 pb-1", divided)}>
        {defs.map((d) => (
          <SettingRow key={d.id} def={d} />
        ))}
      </div>
    </Fold>
  );
}

/** A section's toggles as one checklist, whose changes save one at a time.
 * An item's description that every item shares is left out. */
function Subgroup(props: SectionProps) {
  return (
    <RequestProvider>
      <SubgroupList {...props} />
    </RequestProvider>
  );
}

/** The checklist of a section, inside its request. */
function SubgroupList({ id, title, defs }: SectionProps) {
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
  const header = useHeader(defs);
  const shared = new Set(defs.map((d) => d.description)).size === 1;
  const parents = [...new Set(locks.flatMap((p) => p ?? []))];
  return (
    <Checklist
      id={id}
      title={title}
      {...header}
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
          control: controlId(d),
          label: d.label,
          description: shared ? undefined : description || undefined,
          tags: (
            <>
              {isUntested && <UntestedBadge />}
              <ApBadge mark={apMark(d.archipelago)} />
            </>
          ),
          on: on[i] ?? false,
          changed: drifted[i],
          locked: locks[i] !== undefined,
        };
      })}
      onChange={(changes) => run(() => changeSettings(changes))}
    />
  );
}
