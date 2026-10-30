import * as THREE from 'three';

const VERTEX = /* glsl */ `
attribute vec3 aColor;
attribute float aSize;
attribute float aAlpha;
uniform float uScale;
varying vec3 vColor;
varying float vAlpha;
void main() {
  vColor = aColor;
  vAlpha = aAlpha;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = aAlpha > 0.0 ? aSize * uScale / max(-mv.z, 0.1) : 0.0;
  gl_Position = projectionMatrix * mv;
}`;

const FRAGMENT = /* glsl */ `
varying vec3 vColor;
varying float vAlpha;
void main() {
  vec2 p = gl_PointCoord * 2.0 - 1.0;
  float r2 = dot(p, p);
  if (r2 > 1.0) discard;
  float a = 1.0 - r2;
  a *= a * vAlpha;
  gl_FragColor = vec4(vColor * a, a);
}`;

/** Sparks and flashes for note hits: a ring buffer of additive points, simulated on the CPU. */
export class Particles {
  readonly points: THREE.Points;
  private readonly pos: Float32Array;
  private readonly vel: Float32Array;
  private readonly col: Float32Array;
  private readonly size: Float32Array;
  private readonly base: Float32Array;
  private readonly grow: Float32Array;
  private readonly alpha: Float32Array;
  private readonly age: Float32Array;
  private readonly life: Float32Array;
  private readonly material: THREE.ShaderMaterial;
  private readonly geometry = new THREE.BufferGeometry();
  private next = 0;
  private colorDirty = false;

  constructor(private readonly capacity = 4000) {
    this.pos = new Float32Array(capacity * 3);
    this.vel = new Float32Array(capacity * 3);
    this.col = new Float32Array(capacity * 3);
    this.size = new Float32Array(capacity);
    this.base = new Float32Array(capacity);
    this.grow = new Float32Array(capacity);
    this.alpha = new Float32Array(capacity);
    this.age = new Float32Array(capacity).fill(1);
    this.life = new Float32Array(capacity).fill(1);
    const dynamic = (a: Float32Array, n: number) => new THREE.BufferAttribute(a, n).setUsage(THREE.DynamicDrawUsage);
    this.geometry.setAttribute('position', dynamic(this.pos, 3));
    this.geometry.setAttribute('aColor', dynamic(this.col, 3));
    this.geometry.setAttribute('aSize', dynamic(this.size, 1));
    this.geometry.setAttribute('aAlpha', dynamic(this.alpha, 1));
    this.material = new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: { uScale: { value: 500 } },
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });
    this.points = new THREE.Points(this.geometry, this.material);
    this.points.frustumCulled = false;
    this.points.renderOrder = 20;
  }

  /** Sparks thrown up and toward the player. */
  burst(x: number, y: number, z: number, color: THREE.Color, count: number, speed: number, size: number, life: number): void {
    for (let n = 0; n < count; n++) {
      const u = Math.random() * 2 - 1;
      const phi = Math.random() * Math.PI * 2;
      const r = Math.sqrt(1 - u * u);
      const v = speed * (0.3 + 0.7 * Math.random());
      const white = Math.random() * 0.6;
      this.spawn(
        x, y, z,
        r * Math.cos(phi) * v, (u * 0.7 + 0.6) * v, (r * Math.sin(phi) * 0.5 + 0.45) * v,
        color.r + white, color.g + white, color.b + white,
        size * (0.5 + Math.random()), -0.5, life * (0.5 + 0.7 * Math.random()),
      );
    }
  }

  /** A short glow that swells in place. */
  flash(x: number, y: number, z: number, color: THREE.Color, size: number, life: number): void {
    this.spawn(x, y, z, 0, 0, 0, color.r * 2, color.g * 2, color.b * 2, size, 1.5, life);
  }

  update(dt: number, scale: number): void {
    this.material.uniforms.uScale.value = scale;
    const drag = Math.exp(-2 * dt);
    for (let i = 0; i < this.capacity; i++) {
      if (this.age[i] >= this.life[i]) {
        this.alpha[i] = 0;
        continue;
      }
      this.age[i] += dt;
      const k = Math.min(1, this.age[i] / this.life[i]);
      const j = i * 3;
      this.vel[j + 1] -= 6 * dt;
      this.vel[j] *= drag;
      this.vel[j + 1] *= drag;
      this.vel[j + 2] *= drag;
      this.pos[j] += this.vel[j] * dt;
      this.pos[j + 1] += this.vel[j + 1] * dt;
      this.pos[j + 2] += this.vel[j + 2] * dt;
      this.alpha[i] = Math.pow(1 - k, 1.6);
      this.size[i] = this.base[i] * Math.max(0.05, 1 + this.grow[i] * k);
    }
    const a = this.geometry.attributes;
    a.position.needsUpdate = true;
    a.aAlpha.needsUpdate = true;
    a.aSize.needsUpdate = true;
    if (this.colorDirty) {
      a.aColor.needsUpdate = true;
      this.colorDirty = false;
    }
  }

  private spawn(
    x: number, y: number, z: number,
    vx: number, vy: number, vz: number,
    r: number, g: number, b: number,
    size: number, grow: number, life: number,
  ): void {
    const i = this.next;
    this.next = (this.next + 1) % this.capacity;
    const j = i * 3;
    this.pos.set([x, y, z], j);
    this.vel.set([vx, vy, vz], j);
    this.col.set([r, g, b], j);
    this.base[i] = size;
    this.size[i] = size;
    this.grow[i] = grow;
    this.alpha[i] = 1;
    this.age[i] = 0;
    this.life[i] = life;
    this.colorDirty = true;
  }
}
