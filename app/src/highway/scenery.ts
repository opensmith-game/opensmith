import * as THREE from 'three';
import { drawLogo, type Band } from '../band';
import { t } from '../i18n';
import { glyph } from './labels';
import type { Mood } from './stage';

/**
 * The places behind the highway: real sets in 3D, from the garage to the stadium. The player is
 * on stage and the highway runs toward the audience; around it the room, its props, its crowd and
 * its lights, all following the music (the beat, the crowd's energy, flashes on milestones, red
 * on misses). Built from primitives and textures drawn at load: nothing to download.
 */
export type SceneryId = 'concert' | 'garage' | 'fete' | 'rue' | 'gymnase' | 'bar' | 'club' | 'festival' | 'stade';

export const SCENERIES: readonly { id: SceneryId; label: string }[] = [
  { id: 'concert', label: t('Salle de concert') },
  { id: 'garage', label: 'Garage' },
  { id: 'fete', label: t('Petite fête') },
  { id: 'rue', label: t('Fête de quartier') },
  { id: 'gymnase', label: t('Gymnase') },
  { id: 'bar', label: 'Bar' },
  { id: 'club', label: 'Club' },
  { id: 'festival', label: 'Festival' },
  { id: 'stade', label: t('Stade') },
];

export const isScenery = (x: unknown): x is SceneryId => SCENERIES.some((s) => s.id === x);

/** What a set leaves of the sky behind it: the venue backdrop's crowd and light rig. */
export interface SkyLook {
  crowd: number;
  lights: number;
}

/** A pyrotechnic burst, drawn by the highway's particles. */
export type Pyro = (x: number, y: number, z: number, color: THREE.Color, big: boolean) => void;

type Tick = (t: number, mood: Mood, dim: number) => void;

/** Middle of the neck, where sets centre (the highway spans x 0..24). */
const CX = 12;
/** The room's floor, in the set's frame; the whole set sits DROP lower, the audience below the
 * stage the highway is on. */
const FLOOR = -0.5;
const DROP = -3;
/** The highway's notes come from up to 50 units away: nobody stands in front of them. */
const LANES = { x0: -4, x1: 28, clear: -58 };
const RED = new THREE.Color(1, 0.1, 0.1);
const ALARM = new THREE.Color('#ff2020');

/** Small deterministic generator: a set looks the same every time. */
function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function canvasTexture(w: number, h: number, draw: (g: CanvasRenderingContext2D) => void, repeat: [number, number] = [1, 1]): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  draw(canvas.getContext('2d')!);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
  texture.repeat.set(...repeat);
  texture.anisotropy = 4;
  return texture;
}

const shade = (hex: string, k: number): string => {
  const c = new THREE.Color(hex).multiplyScalar(k);
  return `#${c.getHexString()}`;
};

function bricks(base: string, mortar: string, repeat: [number, number]): THREE.CanvasTexture {
  const rand = random(7);
  return canvasTexture(
    512,
    512,
    (g) => {
      g.fillStyle = mortar;
      g.fillRect(0, 0, 512, 512);
      for (let row = 0; row < 16; row++) {
        const off = row % 2 ? 32 : 0;
        for (let col = -1; col < 8; col++) {
          g.fillStyle = shade(base, 0.75 + rand() * 0.45);
          g.fillRect(col * 64 + off + 3, row * 32 + 3, 58, 26);
        }
      }
    },
    repeat,
  );
}

function planks(base: string, repeat: [number, number], lines = 8): THREE.CanvasTexture {
  const rand = random(11);
  return canvasTexture(
    512,
    512,
    (g) => {
      const h = 512 / lines;
      for (let i = 0; i < lines; i++) {
        g.fillStyle = shade(base, 0.8 + rand() * 0.35);
        g.fillRect(0, i * h, 512, h);
        g.strokeStyle = 'rgba(0,0,0,0.12)';
        for (let k = 0; k < 6; k++) {
          g.beginPath();
          const y = i * h + rand() * h;
          g.moveTo(0, y);
          g.bezierCurveTo(170, y + rand() * 6 - 3, 340, y + rand() * 6 - 3, 512, y);
          g.stroke();
        }
        g.fillStyle = 'rgba(0,0,0,0.5)';
        g.fillRect(0, i * h, 512, 2);
        g.fillRect(rand() * 512, i * h, 2, h);
      }
    },
    repeat,
  );
}

function concrete(base: string, repeat: [number, number]): THREE.CanvasTexture {
  const rand = random(3);
  return canvasTexture(
    256,
    256,
    (g) => {
      g.fillStyle = base;
      g.fillRect(0, 0, 256, 256);
      for (let i = 0; i < 2200; i++) {
        g.fillStyle = `rgba(${rand() < 0.5 ? '0,0,0' : '255,255,255'},${0.03 + rand() * 0.05})`;
        g.fillRect(rand() * 256, rand() * 256, 1 + rand() * 3, 1 + rand() * 3);
      }
      for (let i = 0; i < 6; i++) {
        g.fillStyle = 'rgba(0,0,0,0.08)';
        g.beginPath();
        g.ellipse(rand() * 256, rand() * 256, 10 + rand() * 40, 6 + rand() * 20, rand() * 3, 0, Math.PI * 2);
        g.fill();
      }
    },
    repeat,
  );
}

function wallpaper(a: string, b: string, repeat: [number, number]): THREE.CanvasTexture {
  return canvasTexture(
    256,
    256,
    (g) => {
      g.fillStyle = a;
      g.fillRect(0, 0, 256, 256);
      g.fillStyle = b;
      for (let x = 0; x < 256; x += 64) g.fillRect(x, 0, 22, 256);
      g.fillStyle = 'rgba(255,255,255,0.08)';
      for (let y = 16; y < 256; y += 64) for (let x = 43; x < 256; x += 64) g.fillRect(x - 4, y, 8, 8);
    },
    repeat,
  );
}

/** A gym floor: light planks, the court's lines. */
function court(): THREE.CanvasTexture {
  const wood = planks('#c8955a', [1, 1], 24).image as HTMLCanvasElement;
  return canvasTexture(1024, 1024, (g) => {
    g.drawImage(wood, 0, 0, 1024, 1024);
    g.strokeStyle = 'rgba(255,255,255,0.85)';
    g.lineWidth = 10;
    g.strokeRect(60, 60, 904, 904);
    g.beginPath();
    g.moveTo(60, 512);
    g.lineTo(964, 512);
    g.stroke();
    g.beginPath();
    g.arc(512, 512, 120, 0, Math.PI * 2);
    g.stroke();
    g.strokeStyle = 'rgba(200,40,40,0.8)';
    g.beginPath();
    g.arc(512, 60, 260, 0, Math.PI);
    g.stroke();
  });
}

function turf(): THREE.CanvasTexture {
  return canvasTexture(
    256,
    256,
    (g) => {
      for (let i = 0; i < 8; i++) {
        g.fillStyle = i % 2 ? '#1d5a2a' : '#226632';
        g.fillRect(0, i * 32, 256, 32);
      }
      g.strokeStyle = 'rgba(255,255,255,0.6)';
      g.lineWidth = 3;
      g.strokeRect(4, 4, 248, 248);
    },
    [2, 3],
  );
}

let silhouetteTexture: THREE.CanvasTexture | null = null;

/** A person seen from the stage, head and shoulders and body, in black. */
function silhouette(): THREE.CanvasTexture {
  silhouetteTexture ??= canvasTexture(128, 256, (g) => {
    g.fillStyle = '#ffffff';
    g.beginPath();
    g.arc(64, 44, 26, 0, Math.PI * 2);
    g.fill();
    g.beginPath();
    g.moveTo(14, 256);
    g.lineTo(18, 112);
    g.quadraticCurveTo(22, 82, 64, 80);
    g.quadraticCurveTo(106, 82, 110, 112);
    g.lineTo(114, 256);
    g.fill();
  });
  silhouetteTexture.wrapS = silhouetteTexture.wrapT = THREE.ClampToEdgeWrapping;
  return silhouetteTexture;
}

const BEAM_VERTEX = /* glsl */ `
varying vec2 vUv;
varying vec3 vNormal;
varying vec3 vView;
void main() {
  vUv = uv;
  vec4 world = modelMatrix * vec4(position, 1.0);
  vNormal = normalize(mat3(modelMatrix) * normal);
  vView = normalize(cameraPosition - world.xyz);
  gl_Position = projectionMatrix * viewMatrix * world;
}`;

const BEAM_FRAGMENT = /* glsl */ `
uniform vec3 uColor;
uniform float uIntensity;
varying vec2 vUv;
varying vec3 vNormal;
varying vec3 vView;
void main() {
  // Multisampling can read the varyings just outside the cone: pow() of a negative is NaN, which
  // the bloom then spreads into black blocks.
  float along = pow(clamp(vUv.y, 0.0, 1.0), 1.6);
  float core = pow(abs(dot(normalize(vNormal), vView)), 1.4);
  gl_FragColor = vec4(uColor * along * core * uIntensity, 1.0);
}`;

const LED_FRAGMENT = /* glsl */ `
uniform float uTime;
uniform float uPulse;
uniform float uEnergy;
uniform float uDim;
uniform vec3 uA;
uniform vec3 uB;
varying vec2 vUv;
float hash(float n) { return fract(sin(n * 12.9898) * 43758.5453); }
void main() {
  vec2 cells = vec2(48.0, 20.0);
  vec2 cell = floor(vUv * cells);
  vec2 inCell = fract(vUv * cells);
  float led = smoothstep(0.5, 0.25, length(inCell - 0.5));
  float column = floor(cell.x / 3.0);
  float level = 0.15 + 0.75 * hash(column + floor(uTime * 7.0) * 17.0) * (0.35 + 0.65 * uEnergy) + 0.25 * uPulse;
  float lit = step(cell.y / cells.y, level);
  vec3 col = mix(uA, uB, cell.y / cells.y) * lit;
  float sweep = smoothstep(0.08, 0.0, abs(fract(vUv.x - uTime * 0.15) - 0.5) - 0.42);
  col += uB * sweep * 0.6;
  gl_FragColor = vec4((col * (1.0 + 1.6 * uPulse) + 0.02) * led * uDim * 1.6, 1.0);
}`;

const PLAIN_VERTEX = /* glsl */ `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

/** The band's logo for the sets: on a black cloth, and alone (lit on screens). */
interface Banners {
  cloth: THREE.Texture;
  light: THREE.Texture;
}

/** The pieces sets are made of; each living piece registers its tick. */
class Kit {
  readonly ticks: Tick[] = [];
  readonly rand: () => number;

  constructor(
    readonly root: THREE.Group,
    readonly pyro: Pyro,
    seed: number,
    private readonly banners: Banners | null = null,
  ) {
    this.rand = random(seed);
  }

  /**
   * The band's banner, `w` wide, facing the stage (turned by `ry`): a cloth hung in the room, or
   * the logo lit on a screen (`lift` in front of it). Nothing while the band has no name.
   */
  banner(x: number, y: number, z: number, w: number, o: { ry?: number; light?: boolean; lift?: number } = {}): void {
    if (!this.banners) return;
    const ry = o.ry ?? 0;
    const lift = o.lift ?? 0.3;
    const map = o.light ? this.banners.light : this.banners.cloth;
    const material = new THREE.MeshBasicMaterial({
      map,
      transparent: true,
      depthWrite: !o.light,
      blending: o.light ? THREE.AdditiveBlending : THREE.NormalBlending,
    });
    const image = map.image as HTMLCanvasElement;
    const mesh = this.add(new THREE.Mesh(new THREE.PlaneGeometry(w, (w * image.height) / image.width), material));
    mesh.position.set(x + Math.sin(ry) * lift, y, z + Math.cos(ry) * lift);
    mesh.rotation.y = ry;
    const base = o.light ? 1.4 : 0.8;
    this.ticks.push((_, mood, dim) => {
      material.color.setScalar(base * (0.75 + 0.25 * dim) * (1 + (o.light ? 0.6 * mood.pulse : 0) + mood.flash));
    });
  }

  add<T extends THREE.Object3D>(o: T): T {
    this.root.add(o);
    return o;
  }

  /** A plain colour, or a texture drawn in its own colours (a colour on top would darken it twice). */
  material(look: string | THREE.Texture, rough = 0.85, metal = 0): THREE.MeshStandardMaterial {
    const drawn = look instanceof THREE.Texture;
    return new THREE.MeshStandardMaterial({ color: drawn ? '#ffffff' : look, map: drawn ? look : null, roughness: rough, metalness: metal });
  }

  box(w: number, h: number, d: number, mat: THREE.Material, x: number, y: number, z: number, ry = 0): THREE.Mesh {
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat);
    mesh.position.set(x, y, z);
    mesh.rotation.y = ry;
    return this.add(mesh);
  }

  /** A wall, floor or ceiling: a plane facing `normal` ('up' floor, 'down' ceiling, 'left'/'right' side walls, 'front' back wall). */
  plane(w: number, h: number, mat: THREE.Material, x: number, y: number, z: number, facing: 'up' | 'down' | 'left' | 'right' | 'front'): THREE.Mesh {
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(w, h), mat);
    mesh.position.set(x, y, z);
    if (facing === 'up') mesh.rotation.x = -Math.PI / 2;
    if (facing === 'down') mesh.rotation.x = Math.PI / 2;
    if (facing === 'left') mesh.rotation.y = -Math.PI / 2;
    if (facing === 'right') mesh.rotation.y = Math.PI / 2;
    return this.add(mesh);
  }

  /** The room around the stage: floor, side walls, back wall and ceiling (null: open air). */
  room(o: { w: number; d: number; h: number; floor: THREE.Material; walls: THREE.Material; back?: THREE.Material; ceiling: THREE.Material | null }): void {
    const { w, d, h } = o;
    this.plane(w, d + 40, o.floor, CX, FLOOR, -d / 2 + 20, 'up');
    this.plane(d + 40, h, o.walls, CX - w / 2, FLOOR + h / 2, -d / 2 + 20, 'right');
    this.plane(d + 40, h, o.walls, CX + w / 2, FLOOR + h / 2, -d / 2 + 20, 'left');
    this.plane(w, h, o.back ?? o.walls, CX, FLOOR + h / 2, -d, 'front');
    if (o.ceiling) this.plane(w, d + 40, o.ceiling, CX, FLOOR + h, -d / 2 + 20, 'down');
  }

  /** The stage the player is on, footlights along its front edge (`front`: z of the edge). */
  stage(look: string | THREE.Texture, edge: string, w = 90, front = -24): void {
    const back = 16;
    this.box(w, -DROP, back - front, this.material(look, 0.6), CX, FLOOR - DROP / 2, (back + front) / 2);
    const y = FLOOR - DROP + 0.2;
    this.garland(new THREE.Vector3(CX - w / 2, y, front + 0.5), new THREE.Vector3(CX + w / 2, y, front + 0.5), Math.round(w / 2.2), [edge], 0, 0.16);
  }

  /** Light filling the room (sky above, floor below), dimmed while a song plays, flashing. */
  ambient(sky: string, ground: string, intensity: number): void {
    const light = this.add(new THREE.HemisphereLight(sky, ground, intensity));
    this.ticks.push((_, mood, dim) => {
      light.intensity = intensity * (0.55 + 0.45 * dim) * (1 + 2.5 * mood.flash) * (1 - 0.3 * mood.danger);
    });
  }

  /** A lamp: a glowing bulb (or tube) and the light it sheds, flickering a little, on the beat. */
  lamp(x: number, y: number, z: number, color: string, o: { power?: number; reach?: number; size?: number; beat?: number; flicker?: number; tube?: [number, number] } = {}): void {
    const c = new THREE.Color(color);
    const glow = new THREE.MeshBasicMaterial({ color: c.clone().multiplyScalar(3) });
    const geometry = o.tube ? new THREE.BoxGeometry(o.tube[0], 0.5, o.tube[1]) : new THREE.SphereGeometry(o.size ?? 1, 16, 12);
    const bulb = this.add(new THREE.Mesh(geometry, glow));
    bulb.position.set(x, y, z);
    // A lamp without power only glows: lights cost every lit surface, sets keep a few.
    const base = o.power ?? 3;
    const light = base > 0 ? this.add(new THREE.PointLight(c, base, o.reach ?? 90, 0.5)) : null;
    light?.position.set(x, y - 1, z);
    const phase = this.rand() * 10;
    this.ticks.push((t, mood, dim) => {
      const flicker = 1 - (o.flicker ?? 0) * (Math.sin(t * 23 + phase) * 0.5 + 0.5) * (Math.sin(t * 7.3 + phase) > 0.6 ? 1 : 0);
      const k = flicker * (1 + (o.beat ?? 0.3) * mood.pulse) * (1 + 3 * mood.flash) * (0.6 + 0.4 * dim);
      if (light) light.intensity = base * k;
      glow.color.copy(c).multiplyScalar(3 * k).lerp(RED, 0.4 * mood.danger);
    });
  }

  /** People facing the stage, seated or standing, swaying and jumping with the music. */
  crowd(n: number, place: (i: number, rand: () => number) => { x: number; y: number; z: number; s: number }, tint = '#0a0b12', jump = 1): void {
    if (n <= 0) return;
    const geometry = new THREE.PlaneGeometry(1, 2);
    geometry.translate(0, 1, 0);
    const material = new THREE.MeshBasicMaterial({ map: silhouette(), color: tint, transparent: true, alphaTest: 0.45 });
    const mesh = this.add(new THREE.InstancedMesh(geometry, material, n));
    mesh.frustumCulled = false;
    const spots = Array.from({ length: n }, (_, i) => {
      const p = place(i, this.rand);
      if (p.x > LANES.x0 && p.x < LANES.x1 && p.z > LANES.clear && p.y <= FLOOR) p.z = LANES.clear - this.rand() * 14;
      return { ...p, phase: this.rand() * Math.PI * 2, eager: 0.4 + this.rand() * 0.6 };
    });
    mesh.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(n * 3), 3);
    for (let i = 0; i < n; i++) mesh.setColorAt(i, new THREE.Color().setHSL(0.65 + this.rand() * 0.1, 0.3, 0.05 + this.rand() * 0.07));
    const m = new THREE.Matrix4();
    const size = new THREE.Vector3();
    this.ticks.push((t, mood) => {
      const hop = mood.pulse * (0.15 + 0.85 * mood.energy) * jump;
      spots.forEach((p, i) => {
        const lift = hop * p.eager * p.s * 0.35 + Math.sin(t * 1.3 + p.phase) * 0.04 * p.s;
        const sway = Math.sin(t * 0.9 + p.phase) * 0.05;
        m.makeRotationZ(sway).setPosition(p.x, p.y + Math.max(0, lift), p.z);
        m.scale(size.set(p.s, p.s, p.s));
        mesh.setMatrixAt(i, m);
      });
      mesh.instanceMatrix.needsUpdate = true;
    });
  }

  /** Bulbs on a wire between two points, sagging, twinkling in turn. */
  garland(a: THREE.Vector3, b: THREE.Vector3, n: number, colors: string[], sag = 3, size = 0.45): void {
    const mesh = this.add(new THREE.InstancedMesh(new THREE.SphereGeometry(size, 8, 6), new THREE.MeshBasicMaterial({ color: '#ffffff' }), n));
    mesh.frustumCulled = false;
    const m = new THREE.Matrix4();
    const base = Array.from({ length: n }, (_, i) => new THREE.Color(colors[i % colors.length]));
    for (let i = 0; i < n; i++) {
      const k = (i + 0.5) / n;
      const p = a.clone().lerp(b, k);
      p.y -= sag * 4 * k * (1 - k);
      mesh.setMatrixAt(i, m.makeTranslation(p.x, p.y, p.z));
      mesh.setColorAt(i, base[i]);
    }
    const wire = new THREE.BufferGeometry().setFromPoints(
      Array.from({ length: 24 }, (_, i) => {
        const k = i / 23;
        const p = a.clone().lerp(b, k);
        p.y -= sag * 4 * k * (1 - k);
        return p;
      }),
    );
    this.add(new THREE.Line(wire, new THREE.LineBasicMaterial({ color: '#1a1a1a' })));
    const c = new THREE.Color();
    this.ticks.push((t, mood, dim) => {
      for (let i = 0; i < n; i++) {
        const twinkle = 0.6 + 0.4 * Math.sin(t * 2.4 + i * 1.7);
        c.copy(base[i]).multiplyScalar((1.6 + 1.6 * mood.pulse * mood.energy) * twinkle * (0.65 + 0.35 * dim) * (1 + 2 * mood.flash));
        mesh.setColorAt(i, c);
      }
      mesh.instanceColor!.needsUpdate = true;
    });
  }

  /** A spotlight's beam in the haze, from (x, y, z), swinging around its aim when `swing`. */
  beam(x: number, y: number, z: number, color: string, aim: { tilt: number; pan: number }, o: { length?: number; radius?: number; swing?: number; speed?: number; power?: number; onAt?: number } = {}): void {
    const length = o.length ?? 70;
    const geometry = new THREE.ConeGeometry(o.radius ?? 7, length, 32, 1, true);
    geometry.translate(0, -length / 2, 0);
    const uniforms = { uColor: { value: new THREE.Color(color) }, uIntensity: { value: 0 } };
    const material = new THREE.ShaderMaterial({
      vertexShader: BEAM_VERTEX,
      fragmentShader: BEAM_FRAGMENT,
      uniforms,
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    });
    const cone = new THREE.Mesh(geometry, material);
    const head = new THREE.Group();
    head.position.set(x, y, z);
    head.add(cone);
    this.add(head);
    this.box(1.6, 1.2, 1.6, this.material('#16161a', 0.5, 0.6), x, y + 0.6, z);
    const base = new THREE.Color(color);
    const phase = this.rand() * 10;
    const speed = o.speed ?? 0.5;
    this.ticks.push((t, mood, dim) => {
      const swing = o.swing ?? 0;
      head.rotation.set(aim.tilt + Math.sin(t * speed * 1.3 + phase) * swing * 0.4, aim.pan + Math.sin(t * speed + phase) * swing, 0, 'YXZ');
      const on = mood.energy >= (o.onAt ?? 0) ? 1 : 0.15;
      uniforms.uIntensity.value = (o.power ?? 0.35) * on * (0.6 + 0.8 * mood.pulse + 1.5 * mood.flash) * (0.6 + 0.4 * dim);
      uniforms.uColor.value.copy(base).lerp(ALARM, 0.5 * mood.danger);
    });
  }

  /** A giant LED screen: an equaliser of lights on the beat. */
  led(w: number, h: number, x: number, y: number, z: number, a: string, b: string, ry = 0): void {
    const uniforms = {
      uTime: { value: 0 },
      uPulse: { value: 0 },
      uEnergy: { value: 0 },
      uDim: { value: 1 },
      uA: { value: new THREE.Color(a) },
      uB: { value: new THREE.Color(b) },
    };
    const mesh = this.add(new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.ShaderMaterial({ vertexShader: PLAIN_VERTEX, fragmentShader: LED_FRAGMENT, uniforms })));
    mesh.position.set(x, y, z);
    mesh.rotation.y = ry;
    this.box(w + 1.2, h + 1.2, 0.8, this.material('#0c0c10', 0.6, 0.4), x - Math.sin(ry) * 0.5, y, z - Math.cos(ry) * 0.5, ry);
    this.ticks.push((t, mood, dim) => {
      uniforms.uTime.value = t;
      uniforms.uPulse.value = mood.pulse;
      uniforms.uEnergy.value = mood.energy;
      uniforms.uDim.value = (0.45 + 0.55 * dim) * (1 + mood.flash);
    });
  }

  /** A neon sign: glowing letters, buzzing now and then. */
  neon(text: string, color: string, x: number, y: number, z: number, height: number, ry = 0): void {
    const g = glyph(text, { size: 140, weight: 800, color: '#ffffff', stroke: 'rgba(0,0,0,0)' });
    // A copy: the glyph cache keeps its own texture, this one goes with the set.
    const texture = g.texture.clone();
    texture.needsUpdate = true;
    const c = new THREE.Color(color);
    const material = new THREE.MeshBasicMaterial({ map: texture, color: c.clone().multiplyScalar(2.6), transparent: true, blending: THREE.AdditiveBlending, depthWrite: false });
    const mesh = this.add(new THREE.Mesh(new THREE.PlaneGeometry(height * g.aspect, height), material));
    mesh.position.set(x, y, z);
    mesh.rotation.y = ry;
    const phase = this.rand() * 100;
    this.ticks.push((t, mood, dim) => {
      const buzz = Math.sin(t * 0.7 + phase) > 0.97 && Math.sin(t * 40) > 0 ? 0.25 : 1;
      material.color.copy(c).multiplyScalar(2.6 * buzz * (1 + 0.6 * mood.pulse) * (0.7 + 0.3 * dim));
    });
  }

  /** Lit windows on a facade (x0..x1 along z, or across x), some on, some off, a few changing. */
  windows(points: THREE.Vector3[], facing: number): void {
    const n = points.length;
    const mesh = this.add(new THREE.InstancedMesh(new THREE.PlaneGeometry(3, 4.5), new THREE.MeshBasicMaterial({ color: '#ffffff' }), n));
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion().setFromEuler(new THREE.Euler(0, facing, 0));
    const one = new THREE.Vector3(1, 1, 1);
    const warm = [new THREE.Color('#ffcf7a'), new THREE.Color('#ffb04a'), new THREE.Color('#9fd0ff'), new THREE.Color('#2a2f3c')];
    points.forEach((p, i) => {
      mesh.setMatrixAt(i, m.compose(p, q, one));
      mesh.setColorAt(i, warm[Math.floor(this.rand() * warm.length)].clone().multiplyScalar(1.4));
    });
  }
}

type SetBuilder = (kit: Kit, crowd: number, sign: string | undefined) => { sky: SkyLook | null; fog: [string, number, number] };

const garage: SetBuilder = (kit, crowd) => {
  const w = 110;
  const d = 150;
  const h = 42;
  kit.room({
    w,
    d,
    h,
    floor: kit.material(concrete('#77736b', [10, 14]), 0.6),
    walls: kit.material(bricks('#9a5a3e', '#3a302a', [8, 3])),
    back: kit.material(concrete('#5a5e66', [4, 2]), 0.7, 0.3),
    ceiling: kit.material(planks('#5a4634', [6, 8])),
  });
  // The roll-up door, half open on the night outside.
  const slat = kit.material('#8c9098', 0.5, 0.5);
  for (let i = 0; i < 9; i++) kit.box(70, 2.2, 1, slat, CX, 18 + i * 2.4, -d + 1.5);
  kit.lamp(CX, 15, -d + 3, '#5d7bd6', { power: 2.5, reach: 120, tube: [68, 1], beat: 0.1 });
  kit.banner(CX, 31, -d + 2.4, 34);
  // The bulb on its wire.
  kit.box(0.15, 14, 0.15, kit.material('#111111'), CX, h - 7, -70);
  kit.lamp(CX, h - 14.5, -70, '#ffc472', { power: 9, reach: 150, size: 1.3, flicker: 0.4, beat: 0.5 });
  // A car on the left, the workbench and shelves on the right.
  const paint = kit.material('#7a1d1d', 0.35, 0.6);
  kit.box(16, 6, 36, paint, CX - 38, 3.5, -82);
  kit.box(14, 5, 18, paint, CX - 38, 9, -86);
  kit.box(13.6, 3.6, 0.4, kit.material('#1c2633', 0.1, 0.9), CX - 38, 9.3, -76.8);
  const tyre = kit.material('#151515', 0.9);
  for (const [dx, dz] of [[-8, -70], [8, -70], [-8, -94], [8, -94]]) {
    const wheel = kit.add(new THREE.Mesh(new THREE.CylinderGeometry(3, 3, 2.4, 20), tyre));
    wheel.rotation.z = Math.PI / 2;
    wheel.position.set(CX - 38 + dx, 2.6, dz);
  }
  kit.lamp(CX - 33, 4.5, -63.5, '#fff4d0', { power: 0.6, reach: 30, size: 0.9, beat: 0 });
  kit.lamp(CX - 43, 4.5, -63.5, '#fff4d0', { power: 0.6, reach: 30, size: 0.9, beat: 0 });
  const wood = kit.material(planks('#7a5534', [1, 1]));
  kit.box(10, 1, 40, wood, CX + 46, 9, -70);
  kit.box(1, 9, 40, wood, CX + 46, 4, -70);
  kit.box(2, 14, 40, kit.material(planks('#5a4634', [2, 1])), CX + 53, 23, -70);
  for (let i = 0; i < 12; i++) {
    const c = ['#c94b3a', '#3a6fc9', '#d9b23a', '#5a8a4a'][i % 4];
    kit.box(3 + kit.rand() * 2, 2.5 + kit.rand() * 3, 3, kit.material(c), CX + 50, 11.5 + (kit.rand() < 0.5 ? 0 : 12), -54 - i * 3.2);
  }
  for (let i = 0; i < 4; i++) {
    const t = kit.add(new THREE.Mesh(new THREE.TorusGeometry(2.6, 1.1, 10, 20), tyre));
    t.rotation.x = Math.PI / 2;
    t.position.set(CX + 40, 1 + i * 2.1, -110);
  }
  // An amp stack by the stage.
  const tolex = kit.material('#151515', 0.7);
  kit.box(10, 12, 7, tolex, CX - 22, FLOOR - DROP + 6, -14);
  kit.box(10, 5, 7, kit.material('#1d1d1d', 0.5), CX - 22, FLOOR - DROP + 14.5, -14);
  kit.stage(planks('#8a7254', [5, 2]), '#ffb050', 64, -20);
  kit.ambient('#6a5a50', '#2a2420', 1.2);
  kit.crowd(Math.round(9 * crowd), (i, r) => ({ x: CX + (i % 2 ? 1 : -1) * (10 + r() * 22), y: FLOOR, z: -55 - r() * 45, s: 5.5 + r() * 0.8 }));
  return { sky: null, fog: ['#2a2018', 120, 420] };
};

const fete: SetBuilder = (kit, crowd) => {
  const w = 96;
  const d = 130;
  const h = 36;
  kit.room({
    w,
    d,
    h,
    floor: kit.material(planks('#9a6a3e', [6, 10], 12), 0.5),
    walls: kit.material(wallpaper('#7a5a86', '#8a6a96', [10, 3])),
    ceiling: kit.material('#2a2030'),
  });
  // Garlands along the walls and across the room.
  const colors = ['#ff5a5a', '#ffd25a', '#5ad1ff', '#9dff7a', '#ff7ae0'];
  kit.garland(new THREE.Vector3(CX - 46, h - 4, -10), new THREE.Vector3(CX - 46, h - 4, -125), 46, colors, 2);
  kit.garland(new THREE.Vector3(CX + 46, h - 4, -10), new THREE.Vector3(CX + 46, h - 4, -125), 46, colors, 2);
  for (let i = 0; i < 4; i++) kit.garland(new THREE.Vector3(CX - 46, h - 3, -30 - i * 25), new THREE.Vector3(CX + 46, h - 3, -45 - i * 25), 40, colors, 6);
  // The disco ball and its specks.
  const ball = kit.add(new THREE.Mesh(new THREE.IcosahedronGeometry(4, 2), kit.material('#dfe6ff', 0.15, 1)));
  (ball.material as THREE.MeshStandardMaterial).flatShading = true;
  ball.position.set(CX, h - 9, -70);
  kit.box(0.15, 5, 0.15, kit.material('#222'), CX, h - 2.5, -70);
  kit.lamp(CX, h - 16, -60, '#b48cff', { power: 2.5, reach: 90, size: 0.01, beat: 0.8 });
  kit.ticks.push((t) => {
    ball.rotation.y = t * 0.6;
  });
  kit.beam(CX, h - 9, -70, '#ffe0ff', { tilt: 2.3, pan: 0 }, { length: 50, radius: 2.2, swing: 1.4, speed: 0.6, power: 0.25 });
  kit.beam(CX, h - 9, -70, '#a0e0ff', { tilt: 2.3, pan: 2.1 }, { length: 50, radius: 2.2, swing: 1.4, speed: 0.5, power: 0.25 });
  // The sofa, a lamp, the window, balloons.
  const fabric = kit.material('#4a5a8a', 0.9);
  kit.box(36, 5, 10, fabric, CX, 2.5, -118);
  kit.box(36, 9, 3, fabric, CX, 6, -123);
  kit.box(4, 7, 10, fabric, CX - 18, 4, -118);
  kit.box(4, 7, 10, fabric, CX + 18, 4, -118);
  kit.box(0.6, 14, 0.6, kit.material('#2a2a2a'), CX + 30, 7, -118);
  kit.lamp(CX + 30, 15, -118, '#ffd08a', { power: 2.5, reach: 60, size: 2.2, beat: 0.2 });
  kit.lamp(CX - 26, 20, -d + 0.5, '#4a6ad8', { power: 1.2, reach: 50, tube: [16, 0.2], beat: 0 });
  kit.box(18, 14, 0.4, kit.material('#141c33', 0.2, 0.3), CX - 26, 20, -d + 0.4);
  kit.banner(CX + 2, 22, -d + 0.5, 26);
  for (let i = 0; i < 16; i++) {
    const b = kit.add(new THREE.Mesh(new THREE.SphereGeometry(1.6, 16, 12), kit.material(colors[i % colors.length], 0.3)));
    b.position.set(CX - 40 + kit.rand() * 80, h - 3 - kit.rand() * 3, -20 - kit.rand() * 100);
  }
  kit.stage(planks('#6a4428', [4, 2]), '#ff7ae0', 56, -18);
  kit.ambient('#8a6aa8', '#2a1a20', 1.1);
  kit.crowd(Math.round(16 * crowd), (_, r) => ({ x: CX - 36 + r() * 72, y: FLOOR, z: -45 - r() * 55, s: 5.2 + r() * 0.8 }), '#0a0a12', 1.3);
  return { sky: null, fog: ['#2a1a30', 110, 380] };
};

const rue: SetBuilder = (kit, crowd) => {
  const asphalt = kit.material(concrete('#26282c', [12, 30]), 0.8);
  kit.plane(220, 320, asphalt, CX, FLOOR, -140, 'up');
  // Facades on both sides of the street, their windows lit.
  const facades = ['#9a7a62', '#a88a70', '#7e7a82', '#b09a78', '#8a6a5e'].map((tone) => kit.material(bricks(tone, '#3a3632', [3, 4])));
  const lit: { left: THREE.Vector3[]; right: THREE.Vector3[] } = { left: [], right: [] };
  for (const side of [-1, 1]) {
    let z = -15;
    while (z > -300) {
      const depth = 24 + kit.rand() * 16;
      const height = 34 + kit.rand() * 40;
      const x = CX + side * (58 + 15);
      kit.box(30, height, depth - 1, facades[Math.floor(kit.rand() * facades.length)], x, FLOOR + height / 2, z - depth / 2);
      for (let fy = 8; fy < height - 4; fy += 9) for (let fz = z - 5; fz > z - depth + 3; fz -= 7) (side < 0 ? lit.left : lit.right).push(new THREE.Vector3(x - side * 15.1, fy, fz));
      z -= depth;
    }
  }
  kit.windows(lit.left, Math.PI / 2);
  kit.windows(lit.right, -Math.PI / 2);
  kit.banner(CX, 40, -150, 56);
  // String lights zigzagging over the street, street lamps.
  const colors = ['#ffcf6a', '#ff8a5a', '#fff2c0'];
  for (let i = 0; i < 7; i++) {
    const z = -30 - i * 26;
    kit.garland(new THREE.Vector3(CX - 58, 26, z), new THREE.Vector3(CX + 58, 26, z - 13), 34, colors, 5, 0.55);
  }
  for (let i = 0; i < 5; i++) {
    for (const side of [-1, 1]) {
      const x = CX + side * 52;
      const z = -40 - i * 45;
      kit.box(0.8, 20, 0.8, kit.material('#1c1c20', 0.5, 0.6), x, 10 - 0.5, z);
      kit.lamp(x - side * 2, 20, z, '#ffc070', { power: i < 2 ? 2.2 : 0, reach: 60, size: 1.1, beat: 0.1 });
    }
  }
  kit.stage('#26282e', '#ffcf6a', 76, -22);
  kit.ambient('#3a4a6a', '#1a1a20', 0.8);
  kit.crowd(Math.round(42 * crowd), (_, r) => ({ x: CX - 45 + r() * 90, y: FLOOR, z: -45 - r() * 80, s: 5.4 + r() * 0.8 }));
  return { sky: { crowd: 0, lights: 0 }, fog: ['#141a2c', 140, 480] };
};

const gymnase: SetBuilder = (kit, crowd) => {
  const w = 150;
  const d = 190;
  const h = 64;
  kit.room({
    w,
    d,
    h,
    floor: kit.material(court(), 0.45),
    walls: kit.material(wallpaper('#8a96a8', '#7c889a', [12, 4])),
    ceiling: kit.material('#3a4048'),
  });
  // Fluorescent tubes in rows, the banner, the hoop.
  for (let i = 0; i < 4; i++) {
    for (let k = -1; k <= 1; k++) kit.lamp(CX + k * 40, h - 2, -30 - i * 40, '#e8f2ff', { power: k === 0 && i % 2 === 0 ? 1.6 : 0, reach: 110, tube: [14, 1.4], beat: 0, flicker: i === 2 && k === 1 ? 0.8 : 0 });
  }
  const banner = canvasTexture(1024, 160, (g) => {
    g.fillStyle = '#1b3a8a';
    g.fillRect(0, 0, 1024, 160);
    g.fillStyle = '#ffd85a';
    g.font = '800 92px system-ui, sans-serif';
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText(t('BAL DE FIN D’ANNÉE'), 512, 84);
  });
  kit.plane(70, 11, new THREE.MeshStandardMaterial({ map: banner, roughness: 0.9 }), CX, 46, -d + 0.6, 'front');
  kit.banner(CX, 58, -d + 0.5, 28);
  kit.plane(18, 12, kit.material('#f2f2f2'), CX, 30, -d + 6, 'front');
  kit.box(0.6, 34, 0.6, kit.material('#2a2f3a', 0.4, 0.6), CX, 16, -d + 3);
  const rim = kit.add(new THREE.Mesh(new THREE.TorusGeometry(2.2, 0.2, 8, 24), kit.material('#ff6a1a', 0.4, 0.5)));
  rim.rotation.x = Math.PI / 2;
  rim.position.set(CX, 26, -d + 9);
  // Bleachers along both walls, rising toward them, full.
  const steps = kit.material('#5a6474', 0.7);
  const rowX = (side: number, r: number) => CX + side * (w / 2 - 24 + r * 2.6);
  for (const side of [-1, 1]) for (let r = 0; r < 6; r++) kit.box(2.6, (r + 1) * 3, 130, steps, rowX(side, r), FLOOR + (r + 1) * 1.5, -100);
  kit.crowd(Math.round(150 * crowd), (i, r) => {
    if (i < 70) {
      const row = Math.floor(r() * 6);
      return { x: rowX(i % 2 ? 1 : -1, row), y: FLOOR + (row + 1) * 3, z: -38 - r() * 125, s: 4.4 };
    }
    return { x: CX - 40 + r() * 80, y: FLOOR, z: -45 - r() * 90, s: 5.2 + r() * 0.8 };
  });
  kit.beam(CX - 40, h - 6, -25, '#ff5ad8', { tilt: 2.5, pan: -0.5 }, { length: 70, radius: 6, swing: 0.7, speed: 0.4, power: 0.22, onAt: 0.3 });
  kit.beam(CX + 40, h - 6, -25, '#5ad8ff', { tilt: 2.5, pan: 0.5 }, { length: 70, radius: 6, swing: 0.7, speed: 0.45, power: 0.22, onAt: 0.3 });
  kit.stage('#2a2f3a', '#5ad8ff', 84);
  kit.ambient('#9aa8c8', '#3a3a40', 1.1);
  return { sky: null, fog: ['#2a3040', 140, 500] };
};

const bar: SetBuilder = (kit, crowd, sign) => {
  const w = 120;
  const d = 150;
  const h = 32;
  kit.room({
    w,
    d,
    h,
    floor: kit.material(planks('#4a2e1c', [8, 14], 10), 0.55),
    walls: kit.material(bricks('#8a4c34', '#3a2a1e', [9, 2.5])),
    ceiling: kit.material(planks('#2a1e16', [6, 8])),
  });
  // The bar along the right, its back shelves of lit bottles.
  const wood = kit.material(planks('#4a2a18', [4, 1]), 0.4);
  kit.box(10, 11, 90, wood, CX + 44, FLOOR + 5.5, -95);
  kit.box(12, 0.8, 92, kit.material('#1a0f08', 0.2, 0.3), CX + 44, FLOOR + 11.3, -95);
  for (let row = 0; row < 3; row++) {
    kit.box(3, 0.5, 90, kit.material('#2a1a10'), CX + 57, 14 + row * 5, -95);
    for (let i = 0; i < 26; i++) {
      const bottle = kit.add(new THREE.Mesh(new THREE.CylinderGeometry(0.6, 0.6, 3.4, 10), new THREE.MeshBasicMaterial({ color: new THREE.Color(['#4aa860', '#c87a2a', '#6a8ad8', '#d8c050'][(i + row) % 4]).multiplyScalar(1.4) })));
      bottle.position.set(CX + 57, 16 + row * 5, -55 - i * 3.3);
    }
  }
  kit.lamp(CX + 57, 27, -95, '#ffb060', { power: 2, reach: 50, tube: [1, 88], beat: 0.15 });
  for (let i = 0; i < 6; i++) {
    kit.box(0.5, 8, 0.5, kit.material('#111'), CX + 36, FLOOR + 4, -60 - i * 14);
    kit.box(3, 0.6, 3, kit.material('#7a1a1a', 0.6), CX + 36, FLOOR + 8.3, -60 - i * 14);
  }
  // Tables with candles among the crowd, the neon of the place, spots on the stage.
  for (let i = 0; i < 6; i++) {
    const x = CX - 40 + (i % 3) * 18;
    const z = -95 - Math.floor(i / 3) * 22;
    kit.box(7, 0.6, 7, kit.material('#2a1a10'), x, FLOOR + 7, z);
    kit.box(0.8, 7, 0.8, kit.material('#111'), x, FLOOR + 3.5, z);
    kit.lamp(x, FLOOR + 8, z, '#ff9a3a', { power: 0, size: 0.35, flicker: 0.3, beat: 0 });
  }
  // Pendant lamps over the room.
  for (let k = 0; k < 3; k++) {
    const x = CX - 30 + k * 30;
    kit.box(0.12, 8, 0.12, kit.material('#111'), x, h - 4, -85);
    kit.lamp(x, h - 8.6, -85, '#ffb070', { power: k === 1 ? 4 : 2.5, reach: 90, size: 1, beat: 0.2 });
  }
  kit.neon(sign ?? 'LIVE MUSIC', '#ff3a6a', CX, 20, -d + 0.8, 6);
  kit.neon('♪ LIVE ♪', '#3ad8ff', CX - 45, 19, -d + 0.8, 3.6);
  kit.banner(CX + 33, 19.5, -d + 0.5, 24);
  for (let k = 0; k < 3; k++) kit.beam(CX - 24 + k * 24, h - 1.5, -12, ['#ffd27a', '#ff6a9a', '#7ab0ff'][k], { tilt: 2.75, pan: (k - 1) * 0.35 }, { length: 55, radius: 5, swing: 0.25, speed: 0.3, power: 0.2 });
  kit.stage(planks('#4a2c18', [5, 2]), '#ff3a6a', 74);
  kit.ambient('#8a5a3a', '#201008', 1);
  kit.crowd(Math.round(36 * crowd), (_, r) => ({ x: CX - 42 + r() * 74, y: FLOOR, z: -45 - r() * 75, s: 5.3 + r() * 0.8 }));
  return { sky: null, fog: ['#2a1a10', 100, 380] };
};

/** The LED walls of a club, by the name on its sign: [back wall, side walls]. */
const CLUB_LEDS: [string, string][][] = [
  [['#3a1aff', '#ff2ad8'], ['#1affd0', '#3a6aff']],
  [['#00c8ff', '#2a3aff'], ['#ff2ad8', '#7a2aff']],
  [['#ff6a00', '#ff1a4a'], ['#ffd02a', '#ff6a00']],
  [['#2aff8a', '#00b0ff'], ['#3a1aff', '#2aff8a']],
  [['#ffd02a', '#ff2a6a'], ['#00c8ff', '#ffd02a']],
];

const club: SetBuilder = (kit, crowd, sign) => {
  const w = 160;
  const d = 200;
  const h = 55;
  const [back, sides] = CLUB_LEDS[[...(sign ?? '')].reduce((t, ch) => t + ch.charCodeAt(0), 0) % CLUB_LEDS.length];
  kit.room({ w, d, h, floor: kit.material(concrete('#101018', [10, 14]), 0.25, 0.4), walls: kit.material('#0a0a10'), ceiling: kit.material('#060608') });
  kit.led(120, 28, CX, 23, -d + 1, back[0], back[1]);
  kit.banner(CX, 24, -d + 1, 56, { light: true, lift: 0.6 });
  if (sign) kit.neon(sign, back[1], CX, 41.8, -d + 1.2, 7.5);
  kit.led(40, 24, CX - 74, 26, -100, sides[0], sides[1], Math.PI / 2);
  kit.led(40, 24, CX + 74, 26, -100, sides[0], sides[1], -Math.PI / 2);
  // The truss over the stage and its moving heads.
  const metal = kit.material('#3a3a44', 0.35, 0.8);
  kit.box(120, 1.2, 1.2, metal, CX, h - 8, -12);
  const colors = ['#ff2a6a', '#2ad8ff', '#a82aff', '#ffd02a', '#2aff8a', '#ff7a2a'];
  for (let k = 0; k < 6; k++) kit.beam(CX - 50 + k * 20, h - 9, -12, colors[k], { tilt: 2.35, pan: (k - 2.5) * 0.2 }, { length: 110, radius: 5, swing: 0.9, speed: 0.5 + k * 0.05, power: 0.45, onAt: k < 2 ? 0 : 0.2 * (k - 1) });
  kit.lamp(CX, h - 20, -80, '#5a2aff', { power: 2, reach: 160, size: 0.01, beat: 1 });
  kit.stage('#0e0e14', '#ff2ad8', 120);
  kit.ambient('#2a1a4a', '#06040a', 0.7);
  kit.crowd(Math.round(170 * crowd), (_, r) => ({ x: CX - 70 + r() * 140, y: FLOOR, z: -40 - r() * 140, s: 5.2 + r() * 0.9 }), '#06060c', 1.4);
  return { sky: null, fog: ['#160c28', 90, 380] };
};

const festival: SetBuilder = (kit, crowd, sign) => {
  kit.plane(420, 420, kit.material(concrete('#2e3a24', [20, 20]), 0.95), CX, FLOOR, -180, 'up');
  // Screens on both sides of the stage, delay towers in the field, flags.
  kit.led(36, 22, CX - 46, 22, -20, '#ff8a2a', '#ff2a6a', 0.45);
  kit.led(36, 22, CX + 70, 22, -20, '#ff8a2a', '#ff2a6a', -0.45);
  const metal = kit.material('#5a5a60', 0.4, 0.8);
  kit.banner(CX + 12, 17, -130.5, 44);
  if (sign) kit.neon(sign, '#ffb03a', CX + 12, 28.5, -130.4, 3.6);
  for (const x of [CX - 60, CX + 84]) {
    kit.box(1.2, 44, 1.2, metal, x, 22, -130);
    kit.beam(x, 44, -130, '#ffffff', { tilt: 1.75, pan: x < CX ? -0.5 : 0.5 }, { length: 120, radius: 9, swing: 0.4, speed: 0.25, power: 0.3, onAt: 0.2 });
  }
  for (let i = 0; i < 10; i++) {
    const x = CX - 90 + kit.rand() * 180;
    const z = -70 - kit.rand() * 150;
    kit.box(0.3, 16, 0.3, kit.material('#222'), x, FLOOR + 8, z);
    const flag = kit.add(new THREE.Mesh(new THREE.PlaneGeometry(5, 3.4), new THREE.MeshStandardMaterial({ color: ['#ff4a4a', '#ffd24a', '#4ad2ff', '#ff7ad8'][i % 4], side: THREE.DoubleSide })));
    flag.position.set(x + 2.5, FLOOR + 15, z);
    const phase = kit.rand() * 6;
    kit.ticks.push((t) => {
      flag.rotation.y = Math.sin(t * 2 + phase) * 0.4;
    });
  }
  kit.stage('#16161a', '#ff8a2a', 140, -26);
  kit.ambient('#5a4a7a', '#1a1410', 0.9);
  kit.crowd(Math.round(700 * crowd), (_, r) => {
    const z = -40 - Math.pow(r(), 0.8) * 230;
    const spread = 60 + (-z - 40) * 0.6;
    return { x: CX - spread + r() * 2 * spread, y: FLOOR, z, s: 5.3 + r() * 0.8 };
  }, '#08080e', 1.2);
  let next = 0;
  kit.ticks.push((t, mood) => {
    if (mood.energy > 0.75 && t > next) {
      next = t + 1.5 + Math.random() * 2.5;
      kit.pyro(CX - 60 + Math.random() * 144, 40 + Math.random() * 25, -120 - Math.random() * 80, new THREE.Color().setHSL(Math.random(), 0.9, 0.6), false);
    }
  });
  return { sky: { crowd: 0, lights: 6 }, fog: ['#1e1428', 160, 520] };
};

const stade: SetBuilder = (kit, crowd, sign) => {
  kit.plane(260, 300, kit.material(turf(), 0.9), CX, FLOOR, -150, 'up');
  // The bowl: tiers at the far end and on both sides.
  const concreteTier = kit.material(concrete('#3a3e48', [10, 2]), 0.9);
  for (let r = 0; r < 14; r++) {
    kit.box(320, 3, 8, concreteTier, CX, FLOOR + 1.5 + r * 4, -300 - r * 8);
    for (const side of [-1, 1]) kit.box(8, 3, 280, concreteTier, CX + side * (130 + r * 8), FLOOR + 1.5 + r * 4, -150);
  }
  // Phone lights all over the stands.
  const lights: number[] = [];
  for (let i = 0; i < 2600; i++) {
    const where = kit.rand();
    const r = Math.floor(kit.rand() * 14);
    if (where < 0.5) lights.push(CX - 160 + kit.rand() * 320, FLOOR + 5 + r * 4, -300 - r * 8);
    else lights.push(CX + (where < 0.75 ? -1 : 1) * (130 + r * 8), FLOOR + 5 + r * 4, -20 - kit.rand() * 270);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(lights, 3));
  const phones = new THREE.PointsMaterial({ color: '#cfe6ff', size: 0.9, transparent: true, opacity: 0.9, depthWrite: false, blending: THREE.AdditiveBlending });
  kit.add(new THREE.Points(geometry, phones));
  kit.ticks.push((t, mood) => {
    phones.opacity = (0.35 + 0.65 * mood.energy) * (0.75 + 0.25 * Math.sin(t * 3)) + mood.flash;
  });
  // Light towers and giant screens.
  for (const x of [CX - 150, CX + 174]) {
    kit.box(3, 120, 3, kit.material('#4a4a52', 0.4, 0.8), x, 60, -260);
    kit.lamp(x, 122, -258, '#f4f8ff', { power: 4, reach: 400, tube: [26, 8], beat: 0.1 });
  }
  kit.led(70, 38, CX - 60, 40, -40, '#2a6aff', '#ffffff', 0.35);
  kit.led(70, 38, CX + 84, 40, -40, '#2a6aff', '#ffffff', -0.35);
  // A tifo of lights over the far stand.
  kit.banner(CX, 28, -296, 70, { light: true });
  if (sign) kit.neon(sign, '#9fd0ff', CX, 46, -296, 5);
  const colors = ['#ffffff', '#2ab0ff', '#ff2a6a', '#ffd02a'];
  for (let k = 0; k < 8; k++) kit.beam(CX - 70 + k * 20, 60, -16, colors[k % 4], { tilt: 2.0, pan: (k - 3.5) * 0.15 }, { length: 220, radius: 12, swing: 0.6, speed: 0.35 + k * 0.03, power: 0.35, onAt: k < 2 ? 0 : (k - 1) * 0.1 });
  kit.stage('#121216', '#2ab0ff', 170, -28);
  kit.ambient('#4a5a8a', '#10141a', 0.9);
  kit.crowd(Math.round(1100 * crowd), (_, r) => {
    const z = -40 - Math.pow(r(), 0.7) * 220;
    return { x: CX - 110 + r() * 220, y: FLOOR, z, s: 5.4 + r() * 0.7 };
  }, '#08080e', 1.3);
  let next = 0;
  kit.ticks.push((t, mood) => {
    if ((mood.flash > 0.6 || mood.energy > 0.85) && t > next) {
      next = t + 0.8;
      for (const x of [CX - 36, CX + 60]) kit.pyro(x, FLOOR - DROP, -24, new THREE.Color('#ffb03a'), true);
    }
  });
  return { sky: { crowd: 0, lights: 8 }, fog: ['#0e1424', 200, 600] };
};

const BUILDERS: Record<Exclude<SceneryId, 'concert'>, SetBuilder> = { garage, fete, rue, gymnase, bar, club, festival, stade };

/** The set behind the highway, rebuilt when the place changes. */
export class Scenery {
  private readonly root = new THREE.Group();
  private ticks: Tick[] = [];
  private current = '';
  private time = 0;
  private place: { id: SceneryId; crowd: number; sign?: string } = { id: 'concert', crowd: 1 };
  /** The band's banners, drawn once per band and kept across sets. */
  private banners: Banners | null = null;

  constructor(
    private readonly scene: THREE.Scene,
    private readonly pyro: Pyro,
  ) {
    this.root.position.y = DROP;
    scene.add(this.root);
  }

  /** Builds the place `id` (its crowd scaled by `crowd`, its sign reading `sign`); 'concert': none,
   * the abstract stage of the venue backdrop. Returns what the backdrop shows of its sky. */
  set(id: SceneryId, crowd = 1, sign?: string): SkyLook | null {
    const key = `${id}|${crowd.toFixed(2)}|${sign ?? ''}`;
    if (key === this.current) return this.sky;
    this.current = key;
    this.place = { id, crowd, sign };
    this.clear();
    if (id === 'concert') {
      this.scene.fog = null;
      this.sky = null;
      // The abstract stage: the band's name lit high over the crowd.
      const kit = new Kit(this.root, this.pyro, 1, this.banners);
      kit.banner(CX, 29, -150, 52, { light: true });
      this.ticks = kit.ticks;
      return null;
    }
    const kit = new Kit(this.root, (x, y, z, color, big) => this.pyro(x, y + DROP, z, color, big), id.length * 97 + 13, this.banners);
    const { sky, fog } = BUILDERS[id](kit, crowd, sign);
    this.ticks = kit.ticks;
    this.scene.fog = new THREE.Fog(fog[0], fog[1], fog[2]);
    this.sky = sky ?? { crowd: 0, lights: 0 };
    return this.sky;
  }

  private sky: SkyLook | null = null;

  /** The band whose banner hangs in the sets (null: none), the current set rebuilt with it. */
  setBand(band: Band | null): void {
    if (band) {
      this.banners ??= { cloth: bannerTexture(), light: bannerTexture() };
      drawBanner(this.banners.cloth.image as HTMLCanvasElement, band, true);
      drawBanner(this.banners.light.image as HTMLCanvasElement, band, false);
      this.banners.cloth.needsUpdate = true;
      this.banners.light.needsUpdate = true;
    } else this.banners = null;
    this.current = '';
    this.set(this.place.id, this.place.crowd, this.place.sign);
  }

  update(dt: number, mood: Mood, dim: number): void {
    this.time += dt;
    for (const tick of this.ticks) tick(this.time, mood, dim);
  }

  private clear(): void {
    this.ticks = [];
    this.root.traverse((o) => {
      const mesh = o as THREE.Mesh;
      mesh.geometry?.dispose();
      const materials = Array.isArray(mesh.material) ? mesh.material : mesh.material ? [mesh.material] : [];
      for (const m of materials) {
        for (const value of Object.values(m)) if (value instanceof THREE.Texture && value !== silhouetteTexture && value !== this.banners?.cloth && value !== this.banners?.light) value.dispose();
        m.dispose();
      }
    });
    this.root.clear();
  }
}

function bannerTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 1024;
  canvas.height = 420;
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  return texture;
}

/** The band's logo on `canvas`: on a black cloth with a hem (`cloth`), or alone. */
function drawBanner(canvas: HTMLCanvasElement, band: Band, cloth: boolean): void {
  const ctx = canvas.getContext('2d')!;
  const { width: w, height: h } = canvas;
  ctx.clearRect(0, 0, w, h);
  if (cloth) {
    ctx.fillStyle = '#0e0d12';
    ctx.fillRect(0, 0, w, h);
    // The weave, and the hem with its eyelets.
    ctx.fillStyle = 'rgba(255,255,255,0.025)';
    for (let y = 0; y < h; y += 4) ctx.fillRect(0, y, w, 1);
    ctx.strokeStyle = 'rgba(255,255,255,0.12)';
    ctx.lineWidth = 3;
    ctx.strokeRect(14, 14, w - 28, h - 28);
    ctx.fillStyle = '#9a9aa2';
    for (const x of [26, w / 2, w - 26]) {
      ctx.beginPath();
      ctx.arc(x, 26, 6, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  const logo = document.createElement('canvas');
  logo.width = w - 80;
  logo.height = h - 70;
  drawLogo(logo, band);
  ctx.drawImage(logo, 40, 40);
}
