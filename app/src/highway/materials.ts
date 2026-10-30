import * as THREE from 'three';

/** Uniforms every highway material shares: notes fade in at the far end of the highway. */
export interface Shared {
  uFadeNear: { value: number };
  uFadeFar: { value: number };
}

export const sharedUniforms = (): Shared => ({ uFadeNear: { value: 25 }, uFadeFar: { value: 36 } });

const VERTEX = /* glsl */ `
varying vec3 vColor;
varying vec3 vNormal;
varying vec2 vUv;
varying float vDepth;
varying vec3 vWorld;
void main() {
#ifdef USE_INSTANCING
  mat4 m = modelMatrix * instanceMatrix;
#else
  mat4 m = modelMatrix;
#endif
#ifdef USE_INSTANCING_COLOR
  vColor = instanceColor;
#else
  vColor = vec3(1.0);
#endif
  vec4 world = m * vec4(position, 1.0);
  vDepth = -world.z;
  vWorld = world.xyz;
  vNormal = normalize(mat3(m) * normal);
  vUv = uv;
  gl_Position = projectionMatrix * viewMatrix * world;
}`;

const FRAGMENT = /* glsl */ `
uniform vec3 uTint;
uniform float uShade;
uniform float uEdge;
uniform float uSoft;
uniform float uOpacity;
uniform float uFadeNear;
uniform float uFadeFar;
uniform float uGloss;
varying vec3 vColor;
varying vec3 vNormal;
varying vec2 vUv;
varying float vDepth;
varying vec3 vWorld;
void main() {
  float fade = 1.0 - smoothstep(uFadeNear, uFadeFar, vDepth);
  vec3 n = normalize(vNormal);
  float lambert = 0.42 + 0.58 * max(dot(n, normalize(vec3(0.2, 0.85, 0.5))), 0.0);
  vec2 d = min(vUv, 1.0 - vUv);
  float border = min(d.x, d.y);
  float edge = 1.0 - smoothstep(0.0, 0.16, border);
  float soft = mix(1.0, smoothstep(0.0, 0.5, border * 2.0), uSoft);
  vec3 base = vColor * uTint;
  vec3 col = base * mix(1.0, lambert, uShade) + uEdge * edge * (0.3 + 0.7 * base);
  if (uGloss > 0.0) {
    vec3 view = normalize(cameraPosition - vWorld);
    vec3 light = normalize(vec3(0.2, 0.85, 0.5));
    float spec = pow(max(dot(n, normalize(light + view)), 0.0), 48.0);
    float rim = pow(clamp(1.0 - dot(n, view), 0.0, 1.0), 3.0);
    col += uGloss * (vec3(spec) * 0.9 + base * rim * 0.9);
  }
  float k = fade * soft;
  gl_FragColor = vec4(col * k, uOpacity * k);
}`;

export interface GlowOptions {
  /** Additive blending, no depth writes: light, not matter. */
  additive?: boolean;
  /** 0 = flat colour, 1 = lit like a solid. */
  shade?: number;
  /** Brightness added along the face borders (bevel look). */
  edge?: number;
  /** 1 = soft falloff toward the borders (glows on the floor). */
  soft?: number;
  opacity?: number;
  tint?: [number, number, number];
  /** Specular highlight and rim light (glossy gems). */
  gloss?: number;
}

/** Material for instanced highway pieces: per-instance colour (may exceed 1 to feed the bloom). */
export function glowMaterial(shared: Shared, o: GlowOptions = {}): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: VERTEX,
    fragmentShader: FRAGMENT,
    uniforms: {
      ...shared,
      uTint: { value: new THREE.Color(...(o.tint ?? [1, 1, 1])) },
      uShade: { value: o.shade ?? 0 },
      uEdge: { value: o.edge ?? 0 },
      uSoft: { value: o.soft ?? 0 },
      uOpacity: { value: o.opacity ?? 1 },
      uGloss: { value: o.gloss ?? 0 },
    },
    transparent: !!o.additive || (o.opacity ?? 1) < 1,
    blending: o.additive ? THREE.AdditiveBlending : THREE.NormalBlending,
    depthWrite: !o.additive,
  });
}

/** Floor of the highway: alternating fret lanes, fret wires running into the distance. */
export function floorMaterial(shared: Shared): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: { ...shared, uFrets: { value: 24 } },
    vertexShader: /* glsl */ `
      varying vec3 vWorld;
      void main() {
        vec4 w = modelMatrix * vec4(position, 1.0);
        vWorld = w.xyz;
        gl_Position = projectionMatrix * viewMatrix * w;
      }`,
    fragmentShader: /* glsl */ `
      uniform float uFadeNear;
      uniform float uFadeFar;
      uniform float uFrets;
      varying vec3 vWorld;
      void main() {
        float depth = -vWorld.z;
        float fade = 1.0 - smoothstep(uFadeNear, uFadeFar * 1.15, depth);
        float x = vWorld.x;
        float inside = step(0.0, x) * step(x, uFrets);
        float alt = mod(floor(x), 2.0);
        vec3 col = mix(vec3(0.018, 0.022, 0.04), vec3(0.026, 0.032, 0.056), alt) * inside;
        float fx = abs(fract(x + 0.5) - 0.5);
        float px = fwidth(x);
        float wire = 1.0 - smoothstep(0.012, 0.012 + px * 1.5, fx);
        col += wire * inside * vec3(0.045, 0.055, 0.08);
        // a soft light pool at the strike line
        col += vec3(0.02, 0.035, 0.07) * exp(-abs(depth) * 0.6) * inside;
        gl_FragColor = vec4(col, fade);
      }`,
    transparent: true,
    depthWrite: false,
  });
}

const M = new THREE.Matrix4();
const Q = new THREE.Quaternion();
const P = new THREE.Vector3();
const S = new THREE.Vector3();
const UP = new THREE.Vector3(0, 1, 0);

/** An instanced mesh refilled every frame: begin(), add() per piece, end(). */
export class Layer {
  readonly mesh: THREE.InstancedMesh;
  private n = 0;

  constructor(
    geometry: THREE.BufferGeometry,
    material: THREE.Material,
    private readonly capacity: number,
    order = 0,
  ) {
    this.mesh = new THREE.InstancedMesh(geometry, material, capacity);
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.mesh.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(capacity * 3), 3);
    this.mesh.instanceColor.setUsage(THREE.DynamicDrawUsage);
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = order;
    this.mesh.count = 0;
  }

  begin(): void {
    this.n = 0;
  }

  /** Box or plane centred at (x, y, z), scaled (sx, sy, sz), colour × k, turned by `yaw` about y. */
  add(x: number, y: number, z: number, sx: number, sy: number, sz: number, color: THREE.Color, k = 1, yaw = 0): void {
    if (this.n >= this.capacity) return;
    P.set(x, y, z);
    S.set(sx, sy, sz);
    if (yaw) Q.setFromAxisAngle(UP, yaw);
    else Q.identity();
    M.compose(P, Q, S);
    this.mesh.setMatrixAt(this.n, M);
    this.mesh.instanceColor!.setXYZ(this.n, color.r * k, color.g * k, color.b * k);
    this.n++;
  }

  end(): void {
    const { instanceMatrix, instanceColor } = this.mesh;
    this.mesh.count = this.n;
    instanceMatrix.clearUpdateRanges();
    instanceMatrix.addUpdateRange(0, this.n * 16);
    instanceMatrix.needsUpdate = true;
    instanceColor!.clearUpdateRanges();
    instanceColor!.addUpdateRange(0, this.n * 3);
    instanceColor!.needsUpdate = true;
  }
}

/** Patterns of the fingerboard (Guitar.pattern), as the shader numbers them. */
export const BOARD_PATTERNS = ['grain', 'burst', 'flames', 'neon', 'stars', 'worn', 'lava'] as const;

/**
 * The fingerboard and headstock: wood grain between two tones, or a sunburst, painted flames,
 * lines of light on the beat, a starry night, wood worn bare, molten cracks (uPattern, as in
 * BOARD_PATTERNS).
 */
export function woodMaterial(): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      uA: { value: new THREE.Color() },
      uB: { value: new THREE.Color() },
      uPaint: { value: new THREE.Color() },
      uPattern: { value: 0 },
      uTime: { value: 0 },
      uPulse: { value: 0 },
    },
    vertexShader: /* glsl */ `
      varying vec3 vWorld;
      void main() {
        vec4 w = modelMatrix * vec4(position, 1.0);
        vWorld = w.xyz;
        gl_Position = projectionMatrix * viewMatrix * w;
      }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uA;
      uniform vec3 uB;
      uniform vec3 uPaint;
      uniform float uPattern;
      uniform float uTime;
      uniform float uPulse;
      varying vec3 vWorld;
      float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
      float noise(vec2 p) {
        vec2 i = floor(p);
        vec2 f = fract(p);
        f = f * f * (3.0 - 2.0 * f);
        return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + 1.0), f.x), f.y);
      }
      void main() {
        float x = vWorld.x;
        float z = vWorld.z;
        float grain = 0.5 + 0.5 * sin(z * 70.0 + 2.0 * sin(x * 2.3) + sin(x * 0.6 + z * 9.0));
        // Across the board: 0 at its far edge, 1 at its near edge.
        float across = clamp(z + 0.45, 0.0, 1.0);
        vec3 col = mix(uA, uB, grain * 0.7);
        vec3 light = vec3(0.0);
        if (uPattern > 0.5 && uPattern < 1.5) {
          col = mix(uA, uB, smoothstep(0.05, 0.9, 1.0 - abs(across - 0.5) * 2.0)) * (0.85 + 0.15 * grain);
        } else if (uPattern > 1.5 && uPattern < 2.5) {
          // Tongues of fire from the nut up the neck, their edges flickering.
          float k = floor(across * 3.0);
          float v = abs(fract(across * 3.0) - 0.5) * 2.0;
          float reach = 3.5 + 5.0 * hash(vec2(k, 3.0));
          float edge = reach * (1.0 - v * v) - 0.6 + 0.35 * sin(x * 2.2 + k * 2.0 + uTime * 3.0);
          float fire = smoothstep(0.0, -0.4, x - edge) * step(-0.06, x);
          vec3 flame = mix(vec3(0.75, 0.42, 0.06), uPaint * 0.7, smoothstep(0.0, 0.5, x / reach));
          flame = mix(flame, vec3(0.4, 0.03, 0.01), smoothstep(0.6, 1.0, x / reach));
          col = mix(col, flame, fire);
        } else if (uPattern > 2.5 && uPattern < 3.5) {
          // Lines of light along both edges, a spark running up the neck, all on the beat.
          float lines = exp(-abs(across - 0.05) * 55.0) + exp(-abs(across - 0.95) * 55.0);
          float spark = exp(-abs(fract(x / 24.0 - uTime * 0.2) - 0.5) * 30.0) * 0.6;
          light = uPaint * (lines * (1.2 + 2.2 * uPulse) + spark * lines * 3.0);
        } else if (uPattern > 5.5) {
          // Black rock, cracks of lava glowing with the beat.
          float a = sin(x * 2.3 + sin(z * 9.0) * 1.8) * sin(z * 17.0 + sin(x * 1.3) * 2.2);
          float crack = smoothstep(0.1, 0.0, abs(a)) * (0.6 + 0.4 * sin(x * 0.7 + uTime * 1.5));
          col = mix(uA, uB, grain * 0.5);
          light = uPaint * crack * (0.7 + 1.3 * uPulse);
        } else if (uPattern > 4.5) {
          // Bare wood where the fingers play most, the first frets.
          float wear = smoothstep(0.5, 0.72, noise(vec2(x * 2.2, z * 7.0)) * 0.7 + noise(vec2(x * 6.0, z * 18.0)) * 0.3);
          wear *= (1.0 - smoothstep(4.0, 9.0, x)) * smoothstep(0.0, 0.25, across) * smoothstep(1.0, 0.75, across);
          col = mix(col, uPaint * (0.8 + 0.2 * grain), wear * 0.85);
        } else if (uPattern > 3.5) {
          col = mix(uA, uB, clamp(across * 0.8 + 0.2 * sin(x * 0.35), 0.0, 1.0));
          vec2 cell = floor(vec2(x, z) * 14.0);
          float h = hash(cell);
          float star = step(0.94, h) * smoothstep(0.32, 0.0, length(fract(vec2(x, z) * 14.0) - 0.5));
          light = uPaint * star * (0.6 + 0.6 * sin(uTime * 3.0 + h * 50.0)) * 1.6;
        }
        float front = smoothstep(-0.5, 0.0, z);
        gl_FragColor = vec4(col * (0.45 + 0.55 * front) + light, 1.0);
      }`,
  });
}
