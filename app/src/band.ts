/**
 * The player's band: a name and a logo (an emblem, a lettering, colours), drawn by the game on
 * the gig posters, in the album and on a banner in the sets behind the highway.
 */

import { t } from './i18n';

export type Emblem = 'aucun' | 'eclair' | 'etoile' | 'flamme' | 'crane' | 'couronne' | 'coeur' | 'ailes';
export type Lettering = 'metal' | 'rock' | 'punk' | 'gothique' | 'neon';

export interface Band {
  name: string;
  emblem: Emblem;
  lettering: Lettering;
  /** Index in PALETTES. */
  palette: number;
}

export const NAME_MAX = 24;

export const EMBLEMS: readonly { id: Emblem; label: string }[] = [
  { id: 'eclair', label: t('Éclair') },
  { id: 'etoile', label: t('Étoile') },
  { id: 'flamme', label: t('Flamme') },
  { id: 'crane', label: t('Crâne') },
  { id: 'couronne', label: t('Couronne') },
  { id: 'coeur', label: t('Cœur') },
  { id: 'ailes', label: t('Ailes') },
  { id: 'aucun', label: t('Aucun') },
];

export const LETTERINGS: readonly { id: Lettering; label: string; font: string; upper: boolean }[] = [
  { id: 'metal', label: t('Métal'), font: '"Impact", "Haettenschweiler", "Arial Narrow Bold", sans-serif', upper: true },
  { id: 'rock', label: t('Rock'), font: '"Rockwell", "American Typewriter", Georgia, serif', upper: true },
  { id: 'punk', label: t('Punk'), font: '"Marker Felt", "Chalkduster", "Comic Sans MS", cursive', upper: true },
  { id: 'gothique', label: t('Gothique'), font: '"Luminari", "Trattatello", "Copperplate", fantasy', upper: false },
  { id: 'neon', label: t('Néon'), font: '"Futura", "Avenir Next", "Helvetica Neue", sans-serif', upper: false },
];

export const PALETTES: readonly { label: string; fill: string; accent: string }[] = [
  { label: t('Feu'), fill: '#ffcf4a', accent: '#ff4a1a' },
  { label: t('Glace'), fill: '#eaf6ff', accent: '#3aa8ff' },
  { label: t('Sang'), fill: '#ffffff', accent: '#d01a2a' },
  { label: t('Toxique'), fill: '#c8ff3a', accent: '#1f9a42' },
  { label: t('Néon'), fill: '#ff6ae0', accent: '#6a3aff' },
  { label: t('Or'), fill: '#ffe08a', accent: '#b07a1a' },
];

export const defaultBand = (name: string): Band => ({ name, emblem: 'eclair', lettering: 'metal', palette: 0 });

/** A band read back from a save, or null when there is none worth keeping. */
export function sanitizeBand(raw: unknown): Band | null {
  if (!raw || typeof raw !== 'object') return null;
  const b = raw as Partial<Band>;
  const name = typeof b.name === 'string' ? b.name.trim().slice(0, NAME_MAX) : '';
  if (!name) return null;
  return {
    name,
    emblem: EMBLEMS.some((e) => e.id === b.emblem) ? b.emblem! : 'eclair',
    lettering: LETTERINGS.some((l) => l.id === b.lettering) ? b.lettering! : 'metal',
    palette: typeof b.palette === 'number' && b.palette >= 0 && b.palette < PALETTES.length ? Math.floor(b.palette) : 0,
  };
}

/**
 * The logo on `canvas` (cleared, transparent): the emblem over the name, the name as large as
 * it fits, in the band's colours with a dark outline that reads on paper as on black.
 */
export function drawLogo(canvas: HTMLCanvasElement, band: Band): void {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const { width: w, height: h } = canvas;
  ctx.clearRect(0, 0, w, h);
  const colors = PALETTES[band.palette] ?? PALETTES[0];
  const style = LETTERINGS.find((l) => l.id === band.lettering) ?? LETTERINGS[0];
  const hasEmblem = band.emblem !== 'aucun';
  const emblemSize = hasEmblem ? h * 0.38 : 0;
  if (hasEmblem) {
    ctx.save();
    ctx.translate(w / 2, h * 0.24);
    ctx.scale(emblemSize / 2, emblemSize / 2);
    emblem(ctx, band.emblem);
    ctx.lineJoin = 'round';
    ctx.lineWidth = 0.14;
    ctx.strokeStyle = 'rgba(8, 6, 12, 0.9)';
    ctx.stroke();
    const fill = ctx.createLinearGradient(0, -1, 0, 1);
    fill.addColorStop(0, colors.fill);
    fill.addColorStop(1, colors.accent);
    ctx.fillStyle = fill;
    ctx.fill(band.emblem === 'crane' ? 'evenodd' : 'nonzero');
    ctx.restore();
  }

  // The name: as large as the width allows, below the emblem.
  const text = style.upper ? band.name.toUpperCase() : band.name;
  const top = hasEmblem ? h * 0.46 : h * 0.12;
  const room = h - top - h * 0.06;
  let size = room;
  ctx.font = `${style.id === 'metal' || style.id === 'neon' ? 'bold ' : ''}${size}px ${style.font}`;
  const measured = ctx.measureText(text).width;
  if (measured > w * 0.92) size *= (w * 0.92) / measured;
  ctx.font = `${style.id === 'metal' || style.id === 'neon' ? 'bold ' : ''}${size}px ${style.font}`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const y = top + room / 2;
  const fill = ctx.createLinearGradient(0, y - size / 2, 0, y + size / 2);
  fill.addColorStop(0, colors.fill);
  fill.addColorStop(style.id === 'metal' ? 0.5 : 1, style.id === 'metal' ? '#ffffff' : colors.accent);
  if (style.id === 'metal') fill.addColorStop(1, colors.accent);
  ctx.lineJoin = 'round';
  if (style.id === 'neon') {
    ctx.shadowColor = colors.accent;
    ctx.shadowBlur = size * 0.35;
  }
  if (style.id === 'rock') {
    ctx.fillStyle = colors.accent;
    ctx.fillText(text, w / 2 + size * 0.06, y + size * 0.06);
  }
  ctx.lineWidth = Math.max(2, size * (style.id === 'neon' ? 0.05 : 0.1));
  ctx.strokeStyle = style.id === 'neon' ? colors.accent : 'rgba(8, 6, 12, 0.92)';
  if (style.id === 'punk') {
    // Letters stuck on one by one, slightly askew.
    const widths = [...text].map((ch) => ctx.measureText(ch).width);
    let x = w / 2 - widths.reduce((a, b) => a + b, 0) / 2;
    [...text].forEach((ch, i) => {
      ctx.save();
      ctx.translate(x + widths[i] / 2, y);
      ctx.rotate(Math.sin(i * 2.7) * 0.12);
      ctx.strokeText(ch, 0, 0);
      ctx.fillStyle = fill;
      ctx.fillText(ch, 0, 0);
      ctx.restore();
      x += widths[i];
    });
  } else {
    ctx.strokeText(text, w / 2, y);
    ctx.fillStyle = style.id === 'neon' ? colors.fill : fill;
    ctx.fillText(text, w / 2, y);
  }
  ctx.shadowBlur = 0;
}

/** The emblem's outline as a path, in a box from -1 to 1. */
function emblem(ctx: CanvasRenderingContext2D, id: Emblem): void {
  ctx.beginPath();
  const poly = (points: number[]) => {
    ctx.moveTo(points[0], points[1]);
    for (let i = 2; i < points.length; i += 2) ctx.lineTo(points[i], points[i + 1]);
    ctx.closePath();
  };
  switch (id) {
    case 'eclair':
      poly([0.25, -1, -0.55, 0.12, -0.02, 0.12, -0.3, 1, 0.6, -0.22, 0.06, -0.22, 0.35, -1]);
      break;
    case 'etoile': {
      const pts: number[] = [];
      for (let i = 0; i < 10; i++) {
        const r = i % 2 ? 0.42 : 1;
        const a = -Math.PI / 2 + (i * Math.PI) / 5;
        pts.push(Math.cos(a) * r, Math.sin(a) * r * 0.98 + 0.06);
      }
      poly(pts);
      break;
    }
    case 'flamme':
      ctx.moveTo(0, -1);
      ctx.bezierCurveTo(0.2, -0.55, 0.75, -0.35, 0.7, 0.3);
      ctx.bezierCurveTo(0.66, 0.78, 0.3, 1, 0, 1);
      ctx.bezierCurveTo(-0.3, 1, -0.68, 0.78, -0.7, 0.3);
      ctx.bezierCurveTo(-0.72, -0.1, -0.45, -0.2, -0.38, -0.5);
      ctx.bezierCurveTo(-0.18, -0.3, -0.1, -0.2, -0.08, -0.05);
      ctx.bezierCurveTo(0.05, -0.4, -0.05, -0.7, 0, -1);
      ctx.closePath();
      break;
    case 'crane':
      ctx.moveTo(-0.8, 0);
      ctx.bezierCurveTo(-0.85, -1.05, 0.85, -1.05, 0.8, 0);
      ctx.bezierCurveTo(0.78, 0.3, 0.55, 0.4, 0.5, 0.55);
      ctx.lineTo(0.45, 0.95);
      ctx.lineTo(-0.45, 0.95);
      ctx.lineTo(-0.5, 0.55);
      ctx.bezierCurveTo(-0.55, 0.4, -0.78, 0.3, -0.8, 0);
      ctx.closePath();
      // Eyes and nose, cut out.
      ctx.moveTo(-0.08, -0.05);
      ctx.ellipse(-0.33, -0.05, 0.25, 0.22, 0, 0, Math.PI * 2);
      ctx.moveTo(0.58, -0.05);
      ctx.ellipse(0.33, -0.05, 0.25, 0.22, 0, 0, Math.PI * 2);
      ctx.moveTo(0, 0.25);
      ctx.lineTo(0.12, 0.48);
      ctx.lineTo(-0.12, 0.48);
      ctx.closePath();
      break;
    case 'couronne':
      poly([-0.95, 0.75, -0.95, -0.55, -0.48, -0.05, 0, -0.85, 0.48, -0.05, 0.95, -0.55, 0.95, 0.75]);
      break;
    case 'coeur':
      ctx.moveTo(0, 0.95);
      ctx.bezierCurveTo(-1.2, 0.15, -0.95, -1, 0, -0.42);
      ctx.bezierCurveTo(0.95, -1, 1.2, 0.15, 0, 0.95);
      ctx.closePath();
      break;
    case 'ailes':
      for (const side of [-1, 1]) {
        ctx.moveTo(side * 0.08, 0.1);
        ctx.bezierCurveTo(side * 0.4, -0.75, side * 0.9, -0.85, side * 1, -0.6);
        ctx.bezierCurveTo(side * 0.85, -0.35, side * 0.9, -0.1, side * 0.75, 0.05);
        ctx.bezierCurveTo(side * 0.65, 0.25, side * 0.55, 0.3, side * 0.45, 0.45);
        ctx.bezierCurveTo(side * 0.3, 0.4, side * 0.15, 0.35, side * 0.08, 0.1);
        ctx.closePath();
      }
      ctx.moveTo(0.14, 0.12);
      ctx.arc(0, 0.12, 0.14, 0, Math.PI * 2);
      break;
    case 'aucun':
      break;
  }
}

const logos = new Map<string, string>();

/** The logo as an image URL (cached), for the posters and the album. */
export function logoUrl(band: Band, width = 480, height = 200): string {
  const key = `${JSON.stringify(band)}|${width}x${height}`;
  let url = logos.get(key);
  if (!url) {
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    drawLogo(canvas, band);
    url = canvas.toDataURL('image/png');
    logos.set(key, url);
  }
  return url;
}
