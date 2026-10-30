import * as THREE from 'three';

const VERTEX = /* glsl */ `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

const FRAGMENT = /* glsl */ `
uniform float uTime;
uniform float uPulse;
uniform float uEnergy;
uniform float uDanger;
uniform float uFlash;
uniform float uAspect;
uniform float uHorizon;
uniform float uDim;
uniform float uCrowd;
uniform float uRig;
varying vec2 vUv;

float hash(float n) { return fract(sin(n * 127.1) * 43758.5453); }
float hash2(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise1(float x) {
  float i = floor(x);
  float f = fract(x);
  return mix(hash(i), hash(i + 1.0), f * f * (3.0 - 2.0 * f));
}
float noise2(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2(1.0, 0.0)), f.x), mix(hash2(i + vec2(0.0, 1.0)), hash2(i + 1.0), f.x), f.y);
}
float fbm(vec2 p) {
  float v = 0.0;
  float a = 0.5;
  for (int i = 0; i < 3; i++) {
    v += a * noise2(p);
    p = p * 2.07 + 13.1;
    a *= 0.5;
  }
  return v;
}
/** Stage colours: warm tungsten, amber, violet, overdrive red. */
vec3 palette(float k) {
  k = mod(k, 4.0);
  if (k < 1.0) return vec3(1.0, 0.62, 0.18);
  if (k < 2.0) return vec3(0.62, 0.24, 1.0);
  if (k < 3.0) return vec3(1.0, 0.86, 0.62);
  return vec3(1.0, 0.18, 0.12);
}
float capsule(vec2 p, vec2 a, vec2 b, float r) {
  vec2 pa = p - a;
  vec2 ba = b - a;
  float t = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * t) - r;
}
float roundBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

/**
 * Row k of the crowd (0 at the back, 2 at the front), as a distance (negative inside): people of
 * uneven height and build, shoulders and heads, some arms in the air (more as the room heats up,
 * pumping on the beat). Units are screen heights.
 */
float crowdRow(vec2 p, float k, float h) {
  float s = 0.72 + 0.24 * k;
  float n = 24.0 / s;
  float x = p.x * n + k * 0.37;
  float cell = floor(x);
  float d = 1.0;
  // The front rows fill first, the back ones as the venue fills up.
  float fill = clamp(uCrowd * (1.25 - 0.18 * (2.0 - k)), 0.0, 1.0);
  for (int j = -1; j <= 1; j++) {
    float c = cell + float(j);
    float id = c + k * 131.0;
    if (hash(id + 19.0) > fill) continue;
    float cx = (c + 0.5 + (hash(id + 7.0) - 0.5) * 0.7 - k * 0.37) / n;
    float hr = (0.0095 + 0.0045 * hash(id + 11.0)) * s;
    float tall = (hash(id + 23.0) - 0.5) * 0.016 * s;
    float bob = uPulse * uEnergy * 0.016 * s * hash(id + 3.0);
    float b = h - 0.028 - 0.034 * k + tall + bob + 0.012 * (noise1(c * 0.23 + k) - 0.5);
    vec2 q = vec2(p.x - cx, p.y);
    float wide = 1.6 + 0.6 * hash(id + 29.0);
    float body = roundBox(vec2(q.x, q.y - (b - 0.2)), vec2(hr * wide, 0.2), hr * 1.2);
    float head = length(vec2(q.x / 0.84, q.y - (b + hr * 1.2))) - hr;
    float neck = capsule(q, vec2(0.0, b - hr * 0.5), vec2(0.0, b + hr * 0.6), hr * 0.5);
    float person = min(body, min(head, neck));
    float arms = hash(id + 41.0);
    if (arms < 0.1 + 0.45 * uEnergy) {
      float side = hash(id + 43.0) < 0.5 ? -1.0 : 1.0;
      float sway = sin(uTime * (1.1 + hash(id + 47.0)) + id) * hr * 1.1;
      float pump = uPulse * (0.4 + uEnergy) * hr * 1.6;
      vec2 shoulder = vec2(side * hr * wide * 0.8, b - hr * 0.3);
      vec2 hand = vec2(side * hr * (1.8 + 1.4 * hash(id + 53.0)) + sway, b + hr * (4.4 + 1.8 * hash(id + 59.0)) + pump);
      person = min(person, capsule(q, shoulder, hand, hr * 0.42));
      person = min(person, length(q - hand) - hr * 0.42);
      if (arms < 0.03 + 0.17 * uEnergy) {
        vec2 shoulder2 = vec2(-side * hr * wide * 0.8, b - hr * 0.3);
        vec2 hand2 = vec2(-side * hr * 2.3 - sway, b + hr * 4.3 + pump);
        person = min(person, capsule(q, shoulder2, hand2, hr * 0.42));
        person = min(person, length(q - hand2) - hr * 0.42);
      }
    }
    d = min(d, person);
  }
  // Below the shoulders the row is one dark mass of people.
  return min(d, p.y - (h - 0.075 - 0.04 * k));
}

/** A truss beam between two chords, its lattice in between: 1 on the metal. */
float truss(float along, float across, float width) {
  float px = 0.0016;
  float chords = smoothstep(px, 0.0, abs(across)) + smoothstep(px, 0.0, abs(across - width));
  float t = fract(along / width * 0.5);
  float zig = abs(across / width - abs(2.0 * t - 1.0)) * width;
  float lattice = smoothstep(px * 0.8, 0.0, zig) * step(0.0, across) * step(across, width);
  return clamp(chords + lattice * 0.8, 0.0, 1.0);
}

void main() {
  vec2 p = vec2((vUv.x - 0.5) * uAspect, vUv.y);
  float h = uHorizon;
  float y = vUv.y;

  // The haze in the hall, drifting.
  float fog = fbm(vec2(p.x * 2.6 + uTime * 0.03, y * 4.0 - uTime * 0.018));
  float fogLow = fbm(vec2(p.x * 5.0 - uTime * 0.05, y * 9.0 + 3.0));

  // The dark hall, the warm glow of the stage behind the crowd: the crowd is backlit.
  vec3 col = mix(vec3(0.022, 0.016, 0.02), vec3(0.006, 0.005, 0.008), smoothstep(h, 1.0, y));
  vec3 glowTint = mix(vec3(1.0, 0.5, 0.12), vec3(0.6, 0.22, 1.0), 0.5 + 0.5 * sin(uTime * 0.07));
  float g = (y - h + 0.02) * 6.0;
  col += glowTint * exp(-g * g) * (0.16 + 0.32 * uPulse * uEnergy) * (0.7 + 0.6 * fog);

  // The truss across the top, its towers at the sides.
  float top = 0.915;
  float tw = 0.032;
  float metal = truss(p.x + 10.0, y - top, tw) * step(abs(p.x), uAspect * 0.5 - 0.02);
  float side = uAspect * 0.5 - 0.075;
  float towers = truss(y + 10.0, abs(p.x) - side, tw) * step(y, top + tw);
  float steel = max(metal, towers);

  // Moving heads on the truss: beams that sweep through the haze, brighter on the beat.
  vec3 lights = vec3(0.0);
  for (int i = 0; i < 8; i++) {
    float fi = float(i);
    vec2 origin = vec2((fi - 3.5) * 0.115 * uAspect, top);
    float swing = sin(uTime * (0.18 + 0.035 * fi) + fi * 1.9) * 0.5 + (fi - 3.5) * 0.06;
    vec2 dir = vec2(sin(swing), -cos(swing));
    vec2 d = p - origin;
    float along = dot(d, dir);
    float across = abs(d.x * dir.y - d.y * dir.x);
    float width = 0.006 + along * 0.13;
    float core = smoothstep(width, 0.0, across);
    float beam = core * core * smoothstep(0.0, 0.05, along) * exp(-along * 1.8);
    float rank = floor(abs(fi - 3.5));
    float on = step(rank * 2.0 + 1.0, uRig) * step(rank, 0.5 + 3.5 * uEnergy);
    vec3 c = palette(fi * 1.3 + floor(uTime * 0.05));
    // Volumetric: the beam lights the haze it crosses.
    lights += c * beam * on * (0.3 + 0.95 * fog) * (0.45 + 0.9 * uPulse);
    // The lamp itself on the truss.
    lights += c * on * smoothstep(0.012, 0.0, length(d - dir * 0.004)) * (1.2 + uPulse);
  }
  col += lights * (0.4 + 0.8 * uEnergy);
  col = mix(col, vec3(0.05, 0.05, 0.055) + lights * 0.12 + glowTint * 0.03, steel * 0.92);

  // The crowd, back row to front: dark against the stage light, the back rows lost in the haze.
  vec3 scatter = lights * 0.35 + glowTint * 0.06;
  for (int r = 0; r < 3; r++) {
    float k = float(r);
    float dist = crowdRow(p, k, h);
    float inside = smoothstep(0.0011, -0.0011, dist);
    vec3 body = mix(vec3(0.004, 0.004, 0.006), scatter * 0.5 + vec3(0.02, 0.016, 0.02), (2.0 - k) * 0.22);
    // A thin backlit edge along each silhouette, the stage's colour, never white.
    float edge = inside * smoothstep(-0.0028, 0.0, dist) * (0.35 + 0.65 * smoothstep(h - 0.12, h, y));
    col = mix(col, body, inside) + (glowTint * 0.22 + lights * 0.5) * edge * (0.4 + 0.6 * uPulse) * (1.0 - 0.3 * k);
  }

  // Haze in front of everything, thicker low down, lit by the beams.
  float low = smoothstep(h + 0.05, h - 0.25, y);
  col += (vec3(0.05, 0.04, 0.05) + lights * 0.25) * fogLow * low * 0.55;

  col = mix(col, vec3(0.45, 0.02, 0.05), uDanger * 0.4 * smoothstep(1.0, h - 0.1, y));
  col += vec3(1.0, 0.85, 0.6) * uFlash * 0.3 * smoothstep(h - 0.1, 1.0, y);
  gl_FragColor = vec4(col * uDim, 1.0);
}`;

/** What drives the stage, set every frame. */
export interface Mood {
  /** Beat envelope: 1 on the beat, decays until the next. */
  pulse: number;
  /** 0..1: how hot the crowd is (the streak). */
  energy: number;
  /** A miss: red tint, decays. */
  danger: number;
  /** A milestone: white flash, decays. */
  flash: number;
}

/**
 * The venue behind the highway: a screen-sized backdrop riding with the camera, so the horizon,
 * the crowd and the light rig stay put whatever the camera does.
 */
export class Stage {
  readonly mesh: THREE.Mesh;
  private readonly material: THREE.ShaderMaterial;
  private readonly distance = 300;

  constructor(camera: THREE.PerspectiveCamera) {
    this.material = new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: {
        uTime: { value: 0 },
        uPulse: { value: 0 },
        uEnergy: { value: 0.3 },
        uDanger: { value: 0 },
        uFlash: { value: 0 },
        uAspect: { value: 1.6 },
        uHorizon: { value: 0.78 },
        uDim: { value: 1 },
        uCrowd: { value: 1 },
        uRig: { value: 8 },
      },
      depthWrite: false,
    });
    this.mesh = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), this.material);
    this.mesh.position.set(0, 0, -this.distance);
    this.mesh.renderOrder = -10;
    this.mesh.frustumCulled = false;
    camera.add(this.mesh);
    this.resize(camera);
  }

  /** Fills the camera's view at its distance. */
  resize(camera: THREE.PerspectiveCamera): void {
    const h = 2 * this.distance * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2));
    this.mesh.scale.set(h * camera.aspect * 1.02, h * 1.02, 1);
    this.material.uniforms.uAspect.value = camera.aspect;
  }

  /** The venue: share of its crowd present (0..1) and lights in its rig (1..8). */
  setLook(crowd: number, lights: number): void {
    this.material.uniforms.uCrowd.value = crowd;
    this.material.uniforms.uRig.value = lights;
  }

  /** `dim`: the venue's brightness, lowered while a song plays so the highway stands out. */
  update(time: number, mood: Mood, dim = 1): void {
    const u = this.material.uniforms;
    u.uDim.value = dim;
    u.uTime.value = time;
    u.uPulse.value = mood.pulse;
    u.uEnergy.value = mood.energy;
    u.uDanger.value = mood.danger;
    u.uFlash.value = mood.flash;
  }
}
