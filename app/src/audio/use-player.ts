import { useEffect, useRef, useState } from "react";
import { message } from "@/lib/api";
import { schemeUrl } from "@/lib/platform";

/** The backend's decode of track `id`'s song. */
const songUrl = (id: string) => schemeUrl("zootr-audio", `${id}.wav`);

/** Plays one song at a time: `toggle` starts a track's song, or stops it
 * when it's the one playing. The first play of a song waits on its decode. */
export function usePlayer() {
  const audio = useRef<HTMLAudioElement | null>(null);
  const url = useRef<string | null>(null);
  const [playing, setPlaying] = useState<string | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string>();
  /** Counts plays, so a decode that lands after another play was asked
   * for is dropped. */
  const turn = useRef(0);

  const stop = () => {
    audio.current?.pause();
    if (url.current) URL.revokeObjectURL(url.current);
    audio.current = url.current = null;
    setPlaying(null);
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: stops on unmount only
  useEffect(() => stop, []);

  const toggle = async (id: string) => {
    const was = playing;
    const mine = ++turn.current;
    stop();
    if (was === id) return;
    setLoading(id);
    setError(undefined);
    try {
      // Fetched whole into a blob: the scheme serves no byte ranges, which
      // an <audio> source may ask for.
      const response = await fetch(songUrl(id));
      if (!response.ok) throw new Error("This song couldn't be read");
      const blob = await response.blob();
      if (mine !== turn.current) return;
      url.current = URL.createObjectURL(blob);
      const player = new Audio(url.current);
      player.onended = stop;
      audio.current = player;
      await player.play();
      setPlaying(id);
    } catch (e) {
      stop();
      setError(message(e));
    } finally {
      if (mine === turn.current) setLoading(null);
    }
  };

  return { playing, loading, error, toggle };
}
