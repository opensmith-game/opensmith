import * as THREE from 'three';

export interface Glyph {
  texture: THREE.CanvasTexture;
  /** Width / height. */
  aspect: number;
}

export interface TextStyle {
  size?: number;
  weight?: number;
  color?: string;
  stroke?: string;
  /** Fill of a disc drawn behind the text (finger numbers). */
  badge?: string;
}

const FONT = '-apple-system, "SF Pro Display", "Helvetica Neue", Arial, sans-serif';
const cache = new Map<string, Glyph>();

/** Text rendered once to a canvas texture and cached. */
export function glyph(text: string, style: TextStyle = {}): Glyph {
  const key = `${text}\u0000${style.size}|${style.weight}|${style.color}|${style.stroke}|${style.badge}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const size = style.size ?? 96;
  const font = `${style.weight ?? 700} ${size}px ${FONT}`;
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d')!;
  ctx.font = font;
  canvas.width = Math.max(8, Math.ceil(ctx.measureText(text).width + size * 0.5));
  canvas.height = Math.ceil(size * 1.3);
  if (style.badge) canvas.width = canvas.height = Math.max(canvas.width, canvas.height);
  ctx.font = font;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const x = canvas.width / 2;
  const y = canvas.height / 2 + size * 0.05;
  if (style.badge) {
    ctx.fillStyle = style.badge;
    ctx.beginPath();
    ctx.arc(x, canvas.height / 2, canvas.height * 0.46, 0, Math.PI * 2);
    ctx.fill();
  }
  if (style.stroke) {
    ctx.lineJoin = 'round';
    ctx.lineWidth = size * 0.16;
    ctx.strokeStyle = style.stroke;
    ctx.strokeText(text, x, y);
  }
  ctx.fillStyle = style.color ?? '#ffffff';
  ctx.fillText(text, x, y);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  const g = { texture, aspect: canvas.width / canvas.height };
  cache.set(key, g);
  return g;
}

const WHITE = new THREE.Color(1, 1, 1);

/** Sprites reused every frame for text on the highway (fret numbers on notes, chord names…). */
export class LabelPool {
  private readonly sprites: THREE.Sprite[] = [];
  private n = 0;

  constructor(parent: THREE.Object3D, capacity: number, order = 10) {
    const blank = glyph(' ');
    for (let i = 0; i < capacity; i++) {
      const material = new THREE.SpriteMaterial({ map: blank.texture, transparent: true, depthWrite: false });
      const sprite = new THREE.Sprite(material);
      sprite.visible = false;
      sprite.renderOrder = order;
      parent.add(sprite);
      this.sprites.push(sprite);
    }
  }

  begin(): void {
    this.n = 0;
  }

  add(g: Glyph, x: number, y: number, z: number, height: number, opacity = 1, color: THREE.Color = WHITE): void {
    if (this.n >= this.sprites.length || opacity <= 0.01) return;
    const sprite = this.sprites[this.n++];
    const m = sprite.material;
    m.map = g.texture;
    m.opacity = opacity;
    m.color.copy(color);
    sprite.position.set(x, y, z);
    sprite.scale.set(height * g.aspect, height, 1);
    sprite.visible = true;
  }

  end(): void {
    for (let i = this.n; i < this.sprites.length; i++) this.sprites[i].visible = false;
  }
}

/** A bend: a curved arrow going up, with how far (½, 1, 1½…). */
export function bendGlyph(semitones: number): Glyph {
  const amount = semitones === 0.5 ? '½' : Number.isInteger(semitones) ? String(semitones) : `${Math.floor(semitones)}½`;
  const key = `bend\u0000${amount}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const canvas = document.createElement('canvas');
  canvas.width = 220;
  canvas.height = 160;
  const ctx = canvas.getContext('2d')!;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  for (const [color, width] of [['rgba(0,0,0,0.85)', 22], ['#ffe27a', 12]] as const) {
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.moveTo(30, 140);
    ctx.quadraticCurveTo(70, 140, 80, 40);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(52, 52);
    ctx.lineTo(82, 14);
    ctx.lineTo(108, 56);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
  }
  ctx.font = `800 84px ${FONT}`;
  ctx.textBaseline = 'middle';
  ctx.lineWidth = 14;
  ctx.strokeStyle = 'rgba(0,0,0,0.85)';
  ctx.strokeText(amount, 118, 92);
  ctx.fillStyle = '#ffe27a';
  ctx.fillText(amount, 118, 92);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const g = { texture, aspect: canvas.width / canvas.height };
  cache.set(key, g);
  return g;
}
