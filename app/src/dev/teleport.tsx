import { Segmented } from "@/components/ui/segmented";
import { Actors } from "@/dev/actors";
import { Places } from "@/dev/places";
import { DevSection } from "@/dev/section";
import { useRequest } from "@/lib/request";
import { useUi } from "@/stores/ui";

/** Teleport's tabs. */
const TABS: { id: "places" | "actors"; label: string }[] = [
  { id: "places", label: "Places" },
  { id: "actors", label: "Actors" },
];

/** Teleport to an arrival point, or warp beside an actor of the map. */
export function Teleport() {
  return (
    <DevSection title="Teleport">
      <TabSelector />
      <TeleportContent />
    </DevSection>
  );
}

/** The picked tab's list. */
function TeleportContent() {
  const [tab] = useUi.usePath("dev.teleport.tab");
  return tab === "places" ? <Places /> : <Actors />;
}

/** Picks the tab, dropping what the last teleport said. */
function TabSelector() {
  const [tab, setTab] = useUi.usePath("dev.teleport.tab");
  const request = useRequest();

  return (
    <Segmented
      aria-label="Teleport to"
      className="mb-1.5"
      options={TABS}
      value={tab}
      onValueChange={(id) => {
        setTab(id);
        request.clear();
      }}
    />
  );
}
