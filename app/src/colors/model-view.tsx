import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { PaletteView, Picked } from "@/bindings";
import { Segmented } from "@/components/ui/segmented";
import { useUi } from "@/stores/ui";
import { Loading } from "./loading";
import {
  drop,
  frame,
  loadAlbedo,
  loadModel,
  type Model,
  merge,
  type Part,
  setAlbedo,
} from "./model-mesh";

/** Props for {@link ModelView}. */
interface ModelViewProps {
  palette: PaletteView;
  /** The color to paint, as the picker has it now, or null for vanilla. */
  picked: Picked | null;
}

/** Turns, in radians a second, while nobody drags. */
const TURN = 0.6;

/** What the view draws into, for as long as it's open. */
interface Stage {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  controls: OrbitControls;
  /** The model drawn, once one has loaded. */
  shown: Model | null;
}

/** A turning stage in `el`, lit by the sky and one light. Returns it and
 * how to take it down. */
function stage(el: HTMLElement): [Stage, () => void] {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(window.devicePixelRatio);
  el.appendChild(renderer.domElement);
  const scene = new THREE.Scene();
  scene.add(new THREE.HemisphereLight(0xffffff, 0x404050, 1.6));
  const sun = new THREE.DirectionalLight(0xffffff, 1.8);
  sun.position.set(1, 2, 3);
  scene.add(sun);
  const camera = new THREE.PerspectiveCamera(35, 1, 0.01, 100);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.enablePan = false;
  controls.autoRotate = true;
  controls.autoRotateSpeed = (TURN * 60) / (2 * Math.PI);
  controls.addEventListener("start", () => {
    controls.autoRotate = false;
  });
  const size = () => {
    const { clientWidth: w, clientHeight: h } = el;
    renderer.setSize(w, h);
    camera.aspect = w / h || 1;
    camera.updateProjectionMatrix();
  };
  size();
  const watch = new ResizeObserver(size);
  watch.observe(el);
  renderer.setAnimationLoop(() => {
    controls.update();
    renderer.render(scene, camera);
  });
  const s: Stage = { scene, camera, controls, shown: null };
  return [
    s,
    () => {
      renderer.setAnimationLoop(null);
      watch.disconnect();
      controls.dispose();
      if (s.shown) drop(s.shown);
      renderer.dispose();
      renderer.domElement.remove();
    },
  ];
}

/** `next` on `s`: drawn into the model shown where they share shapes, so
 * Link stays put and only the gear changes, or framed afresh. */
function show(s: Stage, next: Model): Model {
  const kept = s.shown && merge(s.shown, next);
  if (kept) {
    s.shown = kept;
    return kept;
  }
  if (s.shown) {
    s.scene.remove(s.shown.group);
    drop(s.shown);
  }
  s.scene.add(next.group);
  s.shown = next;
  s.controls.target.copy(frame(next.group, s.camera));
  s.controls.update();
  return next;
}

/** The palette's model in the bind pose, turning, with its albedos as the
 * seed paints them: drag to orbit, scroll to zoom. Another palette's
 * model swaps in only the shapes that differ. No game shaders. */
export function ModelView({ palette, picked }: ModelViewProps) {
  const view = useUi((s) => s.colors.views[palette.id] ?? 0);
  const setView = (i: number) =>
    useUi.setPath("colors.views", (s) => ({ ...s, [palette.id]: i }));
  const v = Math.min(view, palette.views.length - 1);
  const [failed, setFailed] = useState(false);
  const [loading, setLoading] = useState(true);
  const box = useRef<HTMLDivElement>(null);
  const at = useRef<Stage | null>(null);
  const [model, setModel] = useState<Model | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const [s, close] = stage(el);
    at.current = s;
    return () => {
      at.current = null;
      setModel(null);
      close();
    };
  }, []);

  useEffect(() => {
    let alive = true;
    setFailed(false);
    setLoading(true);
    loadModel(palette.id, v)
      .then((next) => {
        const s = at.current;
        if (alive && s) setModel(show(s, next));
        else drop(next);
      })
      .catch(() => alive && setFailed(true))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [palette.id, v]);

  // Vanilla albedos load once a part; painted ones again as the pick
  // moves, the newest pick only, one round in flight.
  useAlbedos(palette.id, model, false, null);
  useAlbedos(palette.id, model, true, picked);

  return (
    <div className="flex flex-col gap-3">
      <div
        ref={box}
        role="img"
        aria-label={`${palette.label} on its model`}
        className="relative aspect-square w-full cursor-grab overflow-hidden rounded-md border border-line bg-surface-sunken active:cursor-grabbing"
      >
        {loading && <Loading label={`Reading ${palette.label}'s model`} />}
        {failed && (
          <p className="absolute inset-0 grid place-items-center p-4 text-center text-ink-muted text-sm">
            This model couldn't be drawn
          </p>
        )}
      </div>
      {palette.views.length > 1 && (
        <Segmented
          full
          options={palette.views.map((label, id) => ({ id, label }))}
          value={v}
          onValueChange={setView}
        />
      )}
    </div>
  );
}

/** Keeps `model`'s albedos drawn: those the palette paints (`painted`)
 * with `picked`, or the rest as they are, once each. */
function useAlbedos(
  id: string,
  model: Model | null,
  painted: boolean,
  picked: Picked | null,
) {
  const drawn = painted ? "painted" : "vanilla";
  // Outlives renders, so a round in flight sees the newest pick and model.
  const at = useRef({ model, picked, loading: false });
  useEffect(() => {
    const s = at.current;
    Object.assign(s, { model, picked });
    const next = () => {
      const { model: m, picked: asked } = s;
      if (s.loading || !m) return;
      const parts: Part[] = m.parts.filter(
        (p) =>
          p.head.albedo &&
          p.head.painted === painted &&
          (painted || p.drawn !== "vanilla"),
      );
      if (!parts.length) return;
      s.loading = true;
      Promise.all(parts.map((p) => loadAlbedo(id, p, asked).catch(() => null)))
        .then((textures) =>
          textures.forEach((t, i) => {
            const part = parts[i];
            // Skip a part the next model dropped, or painted the other way.
            if (
              part &&
              part.head.painted === painted &&
              s.model?.parts.includes(part)
            ) {
              setAlbedo(part, t);
              part.drawn = drawn;
            } else t?.dispose();
          }),
        )
        .finally(() => {
          s.loading = false;
          if (s.model !== m || s.picked !== asked) next();
        });
    };
    next();
  }, [id, model, painted, picked, drawn]);
}
