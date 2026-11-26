import {
  Clapperboard,
  Loader2,
  Map as MapIcon,
  MessageCircle,
  Mic,
  Music,
  Music2,
  Play,
  RotateCcw,
  Square,
  Swords,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { AudioDraft, Pool, PoolMode, TrackView } from "@/bindings";
import { FolderTabs } from "@/components/folder-tabs";
import { RandomizeButton } from "@/components/randomize-button";
import { RequestError } from "@/components/request-error";
import { SavedSets } from "@/components/saved-sets";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  bandNote,
  heard,
  length,
  POOL_LABELS,
  poolMode,
  setPool,
  setTrack,
  shuffleAll,
  songsFor,
  type TrackMode,
  trackMode,
  VANILLA,
} from "@/lib/audio";
import { readTracks } from "@/lib/catalogs";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { PaneResetButton } from "@/settings/reset-button";
import { useSaveConfig } from "@/settings/use-save-config";
import { PageHeader } from "@/shell/page-header";
import { useConfig } from "@/stores/config";
import { usePlayer } from "./use-player";

const POOL_TABS = (
  [
    { id: "area", icon: MapIcon },
    { id: "battle", icon: Swords },
    { id: "event", icon: Clapperboard },
    { id: "fanfare", icon: Music },
    { id: "ocarina", icon: Music2 },
    { id: "voice-adult", icon: Mic },
    { id: "voice-child", icon: Mic },
    { id: "npc-voice", icon: MessageCircle },
  ] satisfies { id: Pool; icon: typeof Music }[]
).map((p) => ({ ...p, label: POOL_LABELS[p.id] }));

const POOL_MODES: { id: PoolMode; label: string }[] = [
  { id: "vanilla", label: "Vanilla" },
  { id: "shuffle", label: "Shuffle" },
  { id: "random", label: "Random" },
  { id: "chaos", label: "Chaos" },
];

/** A pool's modes: Chaos is the NPC voices' alone. */
const modesOf = (pool: Pool) =>
  POOL_MODES.filter((m) => m.id !== "chaos" || pool === "npc-voice");

const TRACK_MODES: { id: TrackMode; label: string }[] = [
  { id: "pool", label: "Pool" },
  { id: "vanilla", label: "Vanilla" },
  { id: "pick", label: "Pick" },
];

/** What each pool mode does, under the pool's switch. */
const POOL_HINTS: Record<PoolMode, string> = {
  vanilla: "Every track plays its own music.",
  shuffle:
    "The songs trade places: each plays somewhere once. Short ones trade only with songs of a similar length.",
  random:
    "Each track draws any song, repeats allowed. Short ones draw only songs of a similar length.",
  chaos:
    "Each voice keeps all its lines, and each line plays some voice's clip of the same feeling: a laugh is always someone's laugh.",
};

/** The game's music, each pool Vanilla, Shuffle or Random (NPC voices also
 * Chaos) and each track
 * following its pool, kept vanilla, or given a picked song. */
export function AudioPane() {
  const catalog = useCatalog();
  return (
    <RequestProvider>
      <PageHeader
        title="Audio"
        help="audio"
        description="Shuffle the game's music, Link's voice and the NPCs' voices."
        actions={<Actions />}
      />
      <SavedSets
        kind="audio"
        id="audio-saved"
        words={{
          label: "Saved Audio Sets",
          hint: "Picking one loads its music below, replacing it. Save keeps the music below under the name, replacing a set with that name",
          noun: "audio set",
          unit: "choice",
          empty: "All music is vanilla",
          unsaved: "Not saved. The next seed still plays this music",
        }}
      />
      <Pools {...catalog} />
      <RequestError />
    </RequestProvider>
  );
}

/** Every track a seed can change, and why the catalog couldn't be read. */
function useCatalog() {
  const [tracks, setTracks] = useState<TrackView[]>([]);
  const [error, setError] = useState<string>();
  useEffect(() => {
    readTracks()
      .then(setTracks)
      .catch((e) => setError(String(e)));
  }, []);
  return { tracks, error };
}

/** Randomize, and the page's resets. */
function Actions() {
  const store = useSaveConfig();
  const { busy } = useRequest();
  return (
    <div className="flex flex-wrap gap-2">
      <RandomizeButton
        tip="Every pool on Shuffle. Tracks with their own pick keep it"
        disabled={busy}
        onClick={() => store(shuffleAll)}
      />
      <PaneResetButton pane="audio" />
    </div>
  );
}

/** The tracks by pool, each pool's mode on top. */
function Pools({ tracks, error }: ReturnType<typeof useCatalog>) {
  const [draft] = useConfig.usePath("config.audio");
  const [art] = useConfig.usePath("config.game_assets");
  const [game] = useConfig.usePath("config.game");
  const [pool, setPoolTab] = useState<Pool>("area");
  const store = useSaveConfig();
  const { busy } = useRequest();
  const player = usePlayer();
  const listen = art && game !== null;
  const mode = poolMode(draft, pool);

  const panel = (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium text-sm">Pool mode</span>
        <Segmented
          options={modesOf(pool)}
          value={mode}
          onValueChange={(m) => store(setPool(pool, m))}
          disabled={busy}
          aria-label={`${pool} pool mode`}
        />
        <span className="text-ink-muted text-xs">{POOL_HINTS[mode]}</span>
      </div>
      {listen ? (
        player.error && <p className="text-danger text-xs">{player.error}</p>
      ) : (
        <p className="text-ink-muted text-xs">
          Turn on game assets in Settings to listen to each song.
        </p>
      )}
      <ul className="flex flex-col divide-y divide-line">
        {tracks
          .filter((t) => t.pool === pool)
          .map((t) => (
            <TrackRow
              key={t.id}
              track={t}
              catalog={tracks}
              draft={draft}
              player={listen ? player : null}
            />
          ))}
      </ul>
    </div>
  );

  if (error) return <p className="text-danger text-sm">{error}</p>;
  return (
    <FolderTabs
      label="Music pools"
      value={pool}
      onValueChange={setPoolTab}
      tabs={POOL_TABS.map((p) => ({ ...p, panel }))}
    />
  );
}

/** Props for {@link TrackRow}. */
interface TrackRowProps {
  track: TrackView;
  catalog: TrackView[];
  draft: AudioDraft;
  /** Null without game assets: no play button. */
  player: ReturnType<typeof usePlayer> | null;
}

/** One track: its name, play, Pool, Vanilla or Pick, the picked song and
 * its length. */
function TrackRow({ track, catalog, draft, player }: TrackRowProps) {
  const store = useSaveConfig();
  const { busy } = useRequest();
  const mode = trackMode(draft, track.id);
  const songs = songsFor(catalog, track);
  const picked = mode === "pick" ? draft.tracks?.[track.id] : undefined;
  const donor = catalog.find((t) => t.id === picked);
  const shown = donor ?? track;
  const note = donor && bandNote(track, donor);
  const plays = heard(draft, track);
  const setMode = (m: TrackMode) =>
    store(
      setTrack(
        track.id,
        m === "pool" ? undefined : m === "vanilla" ? VANILLA : songs[0]?.id,
      ),
    );
  return (
    <li className="flex flex-col gap-2 px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex min-w-40 flex-1 flex-col">
          <span className="font-medium text-sm">{track.label}</span>
          <span className="text-ink-muted text-xs">
            <code>{length(shown)}</code>
            {track.gives_none && (
              <Tip tip={track.gives_none}>
                <span className="ml-2 text-ink-faint">receives only</span>
              </Tip>
            )}
          </span>
        </div>
        {player && (
          <PlayButton player={player} id={plays} label={track.label} />
        )}
        <Segmented
          options={TRACK_MODES}
          value={mode}
          onValueChange={setMode}
          disabled={busy}
          aria-label={`${track.label}: its pool, vanilla, or a picked song`}
        />
        <div className="w-52">
          {mode === "pick" && (
            <Select
              value={picked}
              disabled={busy}
              onValueChange={(id) => store(setTrack(track.id, id))}
            >
              <SelectTrigger
                className="w-full"
                aria-label={`${track.label}'s song`}
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent align="start">
                {songs.map((s) => (
                  <SelectItem key={s.id} value={s.id}>
                    {s.label}
                    <span className="ml-2 text-ink-faint text-xs">
                      {length(s)}
                      {bandNote(track, s) && " · other length"}
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        </div>
        <Tip tip="Back to its pool">
          <Button
            size="icon"
            variant="ghost"
            aria-label={`Reset ${track.label}`}
            disabled={busy || mode === "pool"}
            onClick={() => store(setTrack(track.id))}
          >
            <RotateCcw />
          </Button>
        </Tip>
      </div>
      {note && <p className="text-ink-muted text-xs">{note}</p>}
    </li>
  );
}

/** Plays or stops the song a row would play. Disabled when it has none to
 * play: vanilla music in sections. */
function PlayButton({
  player,
  id,
  label,
}: {
  player: ReturnType<typeof usePlayer>;
  id: string | null;
  label: string;
}) {
  const on = id !== null && player.playing === id;
  const loading = id !== null && player.loading === id;
  return (
    <Tip
      tip={
        id === null
          ? "It gives no song of its own: pick one to listen"
          : on
            ? "Stop"
            : "Listen"
      }
      wrap
    >
      <Button
        size="icon"
        variant="ghost"
        aria-label={`${on ? "Stop" : "Play"} ${label}`}
        aria-pressed={on}
        disabled={id === null}
        onClick={() => id && void player.toggle(id)}
      >
        {loading ? (
          <Loader2 className="animate-spin" />
        ) : on ? (
          <Square />
        ) : (
          <Play />
        )}
      </Button>
    </Tip>
  );
}
