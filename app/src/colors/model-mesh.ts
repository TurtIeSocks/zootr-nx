import * as THREE from "three";
import type { Picked } from "@/bindings";
import { albedoUrl, modelUrl } from "@/lib/colors";

/** An eye's iris texture and the mask placing it. */
interface Iris {
  albedo: string;
  mask: string;
  /** How wide the iris and mask show, a share of their textures' width,
   * about the centre. */
  size: number;
}

/** One shape as the mesh binary's head lists it. */
interface Head {
  name: string;
  /** Its albedo texture's name, when its material has one. */
  albedo: string | null;
  /** Whether the palette paints that albedo. */
  painted: boolean;
  /** An eye's iris, laid over its albedo (the sclera) where its mask's
   * red channel is set. */
  iris: Iris | null;
  vertices: number;
  indices: number;
}

/** A shape drawn, with what its texture needs. */
export interface Part {
  head: Head;
  material: THREE.MeshLambertMaterial;
}

/** A model's view, ready to add to a scene: its shapes in one group. */
export interface Model {
  group: THREE.Group;
  parts: Part[];
}

/** Shapes the game draws through their albedo's alpha: hair ends,
 * lashes, fur, the child's eye shadow. */
const CUT_OUT = /AlphaMask|Eyelash|Eyebrow|EyeShadow/;

/** Palette `id`'s view `view`, read from the backend: each shape's
 * positions, normals, UVs and triangles, in the bind pose. */
export async function loadModel(id: string, view: number): Promise<Model> {
  const response = await fetch(modelUrl(id, view));
  if (!response.ok) throw new Error(await response.text());
  const buf = await response.arrayBuffer();
  const length = new DataView(buf).getUint32(0, true);
  const heads: Head[] = JSON.parse(
    new TextDecoder().decode(new Uint8Array(buf, 4, length)),
  );
  let at = 4 + length;
  const floats = (count: number) => {
    const out = new Float32Array(buf, at, count);
    at += count * 4;
    return out;
  };
  const group = new THREE.Group();
  const parts = heads.map((head) => {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute(
      "position",
      new THREE.BufferAttribute(floats(head.vertices * 3), 3),
    );
    geometry.setAttribute(
      "normal",
      new THREE.BufferAttribute(floats(head.vertices * 3), 3),
    );
    geometry.setAttribute(
      "uv",
      new THREE.BufferAttribute(floats(head.vertices * 2), 2),
    );
    geometry.setIndex(
      new THREE.BufferAttribute(new Uint32Array(buf, at, head.indices), 1),
    );
    at += head.indices * 4;
    const material = new THREE.MeshLambertMaterial({
      color: head.albedo ? 0xffffff : 0x9a9a9a,
      side: THREE.DoubleSide,
      alphaTest: CUT_OUT.test(head.name) ? 0.5 : 0,
    });
    group.add(new THREE.Mesh(geometry, material));
    return { head, material };
  });
  return { group, parts };
}

const loader = new THREE.TextureLoader();
const images = new THREE.ImageLoader();

/** `part`'s albedo for palette `id`, painted with `picked`: for an eye,
 * its sclera with its iris laid over it. */
export async function loadAlbedo(
  id: string,
  part: Part,
  picked: Picked | null,
) {
  const { albedo, iris } = part.head;
  if (!albedo) return null;
  const texture = iris
    ? await eye(id, albedo, iris)
    : await loader.loadAsync(albedoUrl(id, albedo, picked));
  // The game's UVs count down from the texture's top row.
  texture.flipY = false;
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.wrapS = THREE.RepeatWrapping;
  texture.wrapT = THREE.RepeatWrapping;
  return texture;
}

/** Texture `name`'s texels on a `width` by `height` canvas (its own size
 * by default), drawn `size` as wide about the centre. */
async function texels(
  id: string,
  name: string,
  width = 0,
  height = 0,
  size = 1,
) {
  const image = await images.loadAsync(albedoUrl(id, name, null));
  const canvas = document.createElement("canvas");
  canvas.width = width || image.width;
  canvas.height = height || image.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("no 2D canvas");
  const [w, h] = [canvas.width * size, canvas.height * size];
  context.drawImage(
    image,
    (canvas.width - w) / 2,
    (canvas.height - h) / 2,
    w,
    h,
  );
  return {
    canvas,
    context,
    pixels: context.getImageData(0, 0, canvas.width, canvas.height),
  };
}

/** An eye's sclera `albedo` with `iris` laid over it as the game's shader
 * does: by the mask's red channel, both shrunk to the iris's size. No
 * palette paints an eye. */
async function eye(id: string, albedo: string, iris: Iris) {
  const sclera = await texels(id, albedo);
  const { width, height } = sclera.canvas;
  const [over, mask] = await Promise.all([
    texels(id, iris.albedo, width, height, iris.size),
    texels(id, iris.mask, width, height, iris.size),
  ]);
  const out = sclera.pixels.data;
  for (let i = 0; i < out.length; i += 4) {
    const t = (mask.pixels.data[i] ?? 0) / 255;
    for (let c = 0; c < 3; c++) {
      const a = out[i + c] ?? 0;
      out[i + c] = a + ((over.pixels.data[i + c] ?? 0) - a) * t;
    }
  }
  sclera.context.putImageData(sclera.pixels, 0, 0);
  return new THREE.CanvasTexture(sclera.canvas);
}

/** `part` drawn with `texture`, the one it had let go. */
export function setAlbedo(part: Part, texture: THREE.Texture | null) {
  part.material.map?.dispose();
  part.material.map = texture;
  part.material.needsUpdate = true;
}

/** How much thinner than its next side a model's thinnest must be before
 * it turns that side to the camera: a sword or a shield, not boots. */
const THIN = 1.5;

/** Turns `group`'s thinnest side to the camera (+Z) and stands it on its
 * longest (Y), when that side is clearly the thinnest and doesn't face
 * the camera already: a sword or shield the game lays flat. Anything
 * else stays as the game has it, Link included. */
function stand(group: THREE.Object3D) {
  const size = new THREE.Box3()
    .setFromObject(group)
    .getSize(new THREE.Vector3());
  const [thin, mid, long] = [0, 1, 2].sort(
    (a, b) => size.getComponent(a) - size.getComponent(b),
  ) as [number, number, number];
  const along = (axis: number) => size.getComponent(axis);
  if (thin === 2 || THIN * along(thin) > along(mid)) return;
  const columns = [
    new THREE.Vector3(),
    new THREE.Vector3(),
    new THREE.Vector3(),
  ];
  columns[long]?.set(0, 1, 0);
  columns[thin]?.set(0, 0, 1);
  columns[mid]?.set(1, 0, 0);
  const [x, y, z] = columns as [THREE.Vector3, THREE.Vector3, THREE.Vector3];
  const turn = new THREE.Matrix4().makeBasis(x, y, z);
  // A swap of two axes mirrors the model: flip the middle one back.
  if (turn.determinant() < 0) columns[mid]?.negate();
  group.setRotationFromMatrix(new THREE.Matrix4().makeBasis(x, y, z));
}

/** Frames `group` in `camera`, stood up, looking at its middle. */
export function frame(group: THREE.Object3D, camera: THREE.PerspectiveCamera) {
  stand(group);
  group.updateMatrixWorld(true);
  const sphere = new THREE.Box3()
    .setFromObject(group)
    .getBoundingSphere(new THREE.Sphere());
  const fov = THREE.MathUtils.degToRad(camera.fov);
  const distance = sphere.radius / Math.sin(fov / 2);
  camera.position.copy(sphere.center).add(new THREE.Vector3(0, 0, distance));
  camera.near = distance / 100;
  camera.far = distance * 10;
  camera.updateProjectionMatrix();
  return sphere.center;
}
