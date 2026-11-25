import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { PaletteView, Picked } from "@/bindings";
import { Segmented } from "@/components/ui/segmented";
import {
  frame,
  loadAlbedo,
  loadModel,
  type Model,
  type Part,
  setAlbedo,
} from "./model-mesh";

/** Props for {@link ModelView}. */
interface ModelViewProps {
  palette: PaletteView;
  /** The colour to paint, as the picker has it now, or null for vanilla. */
  picked: Picked | null;
}

/** Turns, in radians a second, while nobody drags. */
const TURN = 0.6;

/** The palette's model in the bind pose, turning, with its albedos as the
 * seed paints them: drag to orbit, scroll to zoom. Lit by the sky and one
 * light, with none of the game's shaders. */
export function ModelView({ palette, picked }: ModelViewProps) {
  const [view, setView] = useState(0);
  const [failed, setFailed] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const [model, setModel] = useState<Model | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new palette starts at its first view
  useEffect(() => setView(0), [palette.id]);
  const v = Math.min(view, palette.views.length - 1);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
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

    let alive = true;
    let shown: Model | null = null;
    setFailed(false);
    loadModel(palette.id, v)
      .then((m) => {
        if (!alive) return;
        shown = m;
        scene.add(m.group);
        controls.target.copy(frame(m.group, camera));
        controls.update();
        setModel(m);
      })
      .catch(() => alive && setFailed(true));
    renderer.setAnimationLoop(() => {
      controls.update();
      renderer.render(scene, camera);
    });
    return () => {
      alive = false;
      setModel(null);
      renderer.setAnimationLoop(null);
      watch.disconnect();
      controls.dispose();
      for (const part of shown?.parts ?? []) setAlbedo(part, null);
      shown?.group.traverse((o) => {
        if (o instanceof THREE.Mesh) {
          o.geometry.dispose();
          o.material.dispose();
        }
      });
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [palette.id, v]);

  // Vanilla albedos load once per model; painted ones again as the pick
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
 * with `picked`, or the rest as they are. */
function useAlbedos(
  id: string,
  model: Model | null,
  painted: boolean,
  picked: Picked | null,
) {
  // Outlives renders, so a round in flight sees the newest pick and model.
  const at = useRef({ model, picked, loading: false });
  useEffect(() => {
    const s = at.current;
    Object.assign(s, { model, picked });
    const next = () => {
      const { model: m, picked: asked } = s;
      if (s.loading || !m) return;
      const parts: Part[] = m.parts.filter(
        (p) => p.head.albedo && p.head.painted === painted,
      );
      s.loading = true;
      Promise.all(parts.map((p) => loadAlbedo(id, p, asked).catch(() => null)))
        .then((textures) =>
          textures.forEach((t, i) => {
            const part = parts[i];
            if (s.model === m && part) setAlbedo(part, t);
            else t?.dispose();
          }),
        )
        .finally(() => {
          s.loading = false;
          if (s.model !== m || s.picked !== asked) next();
        });
    };
    next();
  }, [id, model, painted, picked]);
}
