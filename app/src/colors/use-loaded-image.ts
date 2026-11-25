import { useEffect, useRef, useState } from "react";

/** The newest `url` that has finished loading, with one request in flight
 * at a time: a picker dragged across the pad asks for its latest color,
 * never a queue of every color it passed. */
export function useLoadedImage(url: string | null) {
  const [shown, setShown] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const wanted = useRef(url);
  const loading = useRef(false);

  useEffect(() => {
    wanted.current = url;
    const next = () => {
      const asked = wanted.current;
      if (loading.current || !asked) return;
      loading.current = true;
      const img = new Image();
      const done = (ok: boolean) => {
        loading.current = false;
        setFailed(!ok);
        if (ok) setShown(asked);
        if (wanted.current !== asked) next();
      };
      img.onload = () => done(true);
      img.onerror = () => done(false);
      img.src = asked;
    };
    next();
  }, [url]);

  return { src: shown, failed };
}
