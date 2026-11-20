import { type LucideIcon, Wrench } from "lucide-react";
import type { ReactNode } from "react";
import { ThemeToggle } from "@/components/theme-toggle";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Badge, ChangedDot } from "@/components/ui/badge";
import { MAC } from "@/lib/platform";
import { cn } from "@/lib/utils";
import {
  APP_PANES,
  groupIcon,
  RANDOMIZER_PANES,
  RUN_TABS,
  type Section,
  type Tab,
  TOP_TABS,
  tabLabel,
} from "@/shell/nav";
import { LiveDot, StatusPill } from "@/shell/status-pill";
import { useChecks } from "@/stores/checks";
import { useConfig } from "@/stores/config";
import { useLive } from "@/stores/live";
import { useSeeds } from "@/stores/seeds";
import { openDev, openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { UpdateAlert } from "@/update/update-alert";

/** Props for {@link NavItem}. */
interface NavItemProps {
  icon: LucideIcon;
  label: string;
  current: boolean;
  onClick: () => void;
  /** Trailing meta: a count, a dot or a badge. */
  children?: ReactNode;
}

function NavItem({
  icon: Icon,
  label,
  current,
  onClick,
  children,
}: NavItemProps) {
  return (
    <button
      type="button"
      aria-current={current ? "page" : undefined}
      onClick={onClick}
      className={cn(
        "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left font-medium text-[13.5px] text-ink transition-colors duration-(--duration-fast) ease-out hover:bg-sidebar-hover [&>svg]:size-4 [&>svg]:shrink-0 [&>svg]:stroke-[1.75] [&>svg]:text-ink-muted",
        current &&
          "bg-surface shadow-[var(--shadow-xs),0_0_0_1px_var(--line)] hover:bg-surface [&>svg]:text-primary-ink",
      )}
    >
      <Icon />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {children}
    </button>
  );
}

/** Props for {@link NavGroup}. */
interface NavGroupProps {
  title: string;
  children: ReactNode;
}

function NavGroup({ title, children }: NavGroupProps) {
  return (
    <div className="mt-4">
      <p className="type-overline px-2 pb-1.5">{title}</p>
      {children}
    </div>
  );
}

const meta = "font-[450] text-ink-faint text-xs tabular-nums";

/** Props for {@link TabItem}. */
interface TabItemProps {
  tab: Tab;
  icon: LucideIcon;
}

function TabItem({ tab, icon }: TabItemProps) {
  const [current, setTab] = useUi.usePath("tab");
  return (
    <NavItem
      icon={icon}
      label={tabLabel(tab)}
      current={current === tab}
      onClick={() => setTab(tab)}
    >
      {tab === "Checks" && <ChecksCount />}
      {tab === "Live" && <LiveState />}
    </NavItem>
  );
}

/** The installed seed's checks done, when one is installed. */
function ChecksCount() {
  const [id] = useChecks.usePath("id");
  const [done] = useChecks.usePath("done");
  const [total] = useChecks.usePath("total");
  if (id === null) return null;
  return (
    <span className={meta}>
      {done}/{total}
    </span>
  );
}

function LiveState() {
  const [state] = useLive.usePath("status.state");
  return <LiveDot state={state} />;
}

/** Props for {@link SectionItem}. */
interface SectionItemProps {
  section: Section;
  label: string;
  icon: LucideIcon;
  children?: ReactNode;
}

/** A Settings page, current when it's the one showing. */
function SectionItem({ section, label, icon, children }: SectionItemProps) {
  const [tab] = useUi.usePath("tab");
  const [current] = useUi.usePath("section");
  return (
    <NavItem
      icon={icon}
      label={label}
      current={tab === "Settings" && current === section}
      onClick={() => openSettings(section)}
    >
      {children}
    </NavItem>
  );
}

/** The installed seed's views. Hidden with no seed installed. It redraws
 * only when a seed is installed or removed. */
function RunGroup() {
  const installed = useSeeds((s) => s.history.installed !== null);
  if (!installed) return null;
  return (
    <NavGroup title="Current run">
      {RUN_TABS.map(([t, icon]) => (
        <TabItem key={t} tab={t} icon={icon} />
      ))}
    </NavGroup>
  );
}

/** The schema's groups in order, each marked when it holds a change. */
function Groups() {
  const [groups] = useConfig.usePath("groups");
  return groups.map((g) => (
    <SectionItem
      key={g.name}
      section={g.name}
      label={g.name}
      icon={groupIcon(g.name)}
    >
      {g.changed && (
        <span role="img" aria-label="changed">
          <ChangedDot />
        </span>
      )}
    </SectionItem>
  ));
}

/** How many checks the next seed pins, when it pins any. */
function PlandoOn() {
  const pins = useConfig((s) => Object.keys(s.config.place).length);
  return (
    pins > 0 && (
      <Tip tip={`${pins} pinned`}>
        <Badge variant="info">{pins}</Badge>
      </Tip>
    )
  );
}

function TestOn() {
  const [on] = useConfig.usePath("testOn");
  return on && <Badge variant="info">On</Badge>;
}

/** Whether the game dump and each target are ready. */
function StatusPills() {
  const [status] = useSeeds.usePath("status");
  return (
    <div className="flex min-w-0 flex-1 flex-wrap gap-1.5">
      <StatusPill found={status.game} onClick={() => openSettings("settings")}>
        {status.game ? "Game dump" : "Game dump missing"}
      </StatusPill>
      {status.targets.map((t) => (
        <StatusPill
          key={t.name}
          found={t.app}
          running={t.running}
          onClick={() => openSettings("targets")}
        >
          {t.name}
          {!t.app && " missing"}
          {t.running && <span className="text-ink-faint"> running</span>}
        </StatusPill>
      ))}
    </div>
  );
}

/** This build's version, so a screenshot always says which one it is. */
function Version() {
  const [current] = useUpdate.usePath("current");
  return (
    <span className="mr-auto pl-2 font-mono text-ink-faint text-xs">
      {current && `v${current}`}
    </span>
  );
}

/** Opens the dev panel. Greyed while the setting is off. */
function DevButton() {
  const [dev] = useConfig.usePath("config.dev_panel");
  return (
    <TipButton
      label="Dev panel"
      variant="ghost"
      tip={dev ? "Dev panel" : "The dev panel is off. Click to turn it on"}
      className={cn(!dev && "opacity-45")}
      onClick={openDev}
    >
      <Wrench />
    </TipButton>
  );
}

/** The app's only navigation, running the window's full height. */
export function Sidebar() {
  return (
    <aside className="flex min-h-0 flex-col border-line border-r bg-sidebar">
      <div
        data-tauri-drag-region
        className={cn(
          "flex h-(--titlebar) shrink-0 items-center px-4",
          // Clear of the macOS window buttons; in line with the nav elsewhere.
          MAC && "justify-end",
        )}
      >
        <span data-tauri-drag-region className="type-display-sm text-ink">
          zootr<span className="text-primary-ink">-nx</span>
        </span>
      </div>
      <nav className="min-h-0 flex-1 overflow-auto px-2 pt-1 pb-3">
        {TOP_TABS.map(([t, icon]) => (
          <TabItem key={t} tab={t} icon={icon} />
        ))}
        <RunGroup />
        <NavGroup title="Randomizer">
          <Groups />
          {RANDOMIZER_PANES.map(([id, label, icon]) => (
            <SectionItem key={id} section={id} label={label} icon={icon}>
              {id === "plando" && <PlandoOn />}
            </SectionItem>
          ))}
        </NavGroup>
        <NavGroup title="App">
          {APP_PANES.map(([id, label, icon]) => (
            <SectionItem key={id} section={id} label={label} icon={icon}>
              {id === "test" && <TestOn />}
            </SectionItem>
          ))}
        </NavGroup>
      </nav>
      <div className="flex shrink-0 flex-col gap-2 border-line border-t p-3">
        <UpdateAlert />
        <StatusPills />
        <div className="flex items-center gap-1.5">
          <Version />
          <DevButton />
          <ThemeToggle />
        </div>
      </div>
    </aside>
  );
}
