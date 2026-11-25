import type { ReactNode } from "react";
import { AudioPane } from "@/audio/audio-pane";
import { ColorsPane } from "@/colors/colors-pane";
import { InlineError } from "@/components/callout";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { LogsView } from "@/logs/logs-view";
import { AppSettings } from "@/settings/app-settings";
import { DriftBar } from "@/settings/drift-bar";
import { GroupPage } from "@/settings/group-page";
import { PlandoPane } from "@/settings/plando-pane";
import { TargetsPane } from "@/settings/targets-pane";
import { TestPane } from "@/settings/test-pane";
import { Page } from "@/shell/page-header";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

/** A settings group's page, or one of the app's panes, in the page frame. */
export function SettingsView() {
  const [section] = useUi.usePath("section");
  if (section === "logs") return <LogsView />;
  return (
    <Page>
      {PANES[section] ?? <Group group={section} />}
      <DriftBar />
    </Page>
  );
}

/** The app's own panes, by section. Any other section is a schema group. */
const PANES: Partial<Record<string, ReactNode>> = {
  settings: <AppSettings />,
  targets: <TargetsPane />,
  test: <Test />,
  plando: <PlandoPane />,
  colors: <ColorsPane />,
  audio: <AudioPane />,
};

/** The test setup's form, reset whenever the saved setup changes. */
function Test() {
  const [test] = useConfig.usePath("config.test");
  return <TestPane key={JSON.stringify(test)} />;
}

/** Props for {@link Group}. */
interface GroupProps {
  /** The schema group to show. */
  group: string;
}

/** A schema group's page, then why its reset or a seed action failed. */
function Group({ group }: GroupProps) {
  return (
    <RequestProvider>
      <GroupPage group={group} />
      <GroupError />
    </RequestProvider>
  );
}

/** Why the group's reset failed, or else why the last seed action did. */
function GroupError() {
  const { error } = useRequest();
  const [failed] = useSeeds.usePath("error");
  return <InlineError error={error ?? failed} />;
}
