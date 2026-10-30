/**
 * Guitars: the look of the neck under the strings (fingerboard and headstock, frets and the fret
 * posts over the highway, inlays, nut), bought in the career's shop or won with a trophy, and
 * played in every mode. The strings keep their colours whatever the guitar.
 */
import { t } from './i18n';

export type Pattern = 'grain' | 'burst' | 'flames' | 'neon' | 'stars' | 'worn' | 'lava';
export type InlayShape = 'dots' | 'blocks' | 'sharks' | 'stars' | 'diamonds' | 'none';

export interface Guitar {
  id: string;
  name: string;
  blurb: string;
  /** Price in the shop (€); 0 for the guitar every player starts with and those won. */
  price: number;
  /** Not for sale: the trophy that gives it. */
  trophy?: string;
  /** Fingerboard and headstock: the two tones of the wood or paint, and what is drawn on it. */
  board: [string, string];
  pattern: Pattern;
  /** Colour of what is drawn: flames, lines of light, stars, bare wood, molten cracks. */
  paint?: string;
  frets: string;
  /** The fret posts standing over the neck, where the strings are. */
  posts: string;
  inlays: InlayShape;
  inlay: string;
  /** Inlays that shine. */
  glow?: boolean;
  nut: string;
}

export const DEFAULT_GUITAR = 'palissandre';

export const GUITARS: readonly Guitar[] = [
  {
    id: 'palissandre',
    name: t('Palissandre'),
    blurb: t('La touche de série : palissandre, frettes nickel, points nacrés.'),
    price: 0,
    board: ['#452f22', '#614430'],
    pattern: 'grain',
    frets: '#aaadb8',
    posts: '#adb8ce',
    inlays: 'dots',
    inlay: '#dadde5',
    nut: '#e5dfce',
  },
  {
    id: 'erable',
    name: t('Érable'),
    blurb: t('La touche claire des Strat des années 50, points noirs.'),
    price: 120,
    board: ['#b88e58', '#d8b57c'],
    pattern: 'grain',
    frets: '#b4b6be',
    posts: '#c8ccd6',
    inlays: 'dots',
    inlay: '#16110c',
    nut: '#efe8d6',
  },
  {
    id: 'ebene',
    name: t('Ébène'),
    blurb: t('Noir profond et blocs de nacre, comme une Les Paul Custom.'),
    price: 250,
    board: ['#141110', '#241e1b'],
    pattern: 'grain',
    frets: '#c8cad2',
    posts: '#c4ccdc',
    inlays: 'blocks',
    inlay: '#bdbab2',
    nut: '#f2eee4',
  },
  {
    id: 'sunburst',
    name: 'Sunburst',
    blurb: t('Le dégradé ambre des guitares vintage, frettes dorées.'),
    price: 400,
    board: ['#3a1a08', '#e09a3a'],
    pattern: 'burst',
    frets: '#d8b46a',
    posts: '#e0c27a',
    inlays: 'blocks',
    inlay: '#cdbf9e',
    nut: '#f3ead2',
  },
  {
    id: 'abalone',
    name: 'Abalone',
    blurb: t('Des ailerons de requin en nacre irisée sur un palissandre sombre.'),
    price: 600,
    board: ['#2a1c14', '#3e2a1e'],
    pattern: 'grain',
    frets: '#b8bcc8',
    posts: '#9fd8d0',
    inlays: 'sharks',
    inlay: '#7fe0d0',
    glow: true,
    nut: '#e5dfce',
  },
  {
    id: 'flammes',
    name: t('Flammes'),
    blurb: t('Une peinture hot rod : des flammes qui partent du sillet.'),
    price: 900,
    board: ['#0c0a0a', '#1a1414'],
    pattern: 'flames',
    paint: '#ff6a10',
    frets: '#d8dce6',
    posts: '#ff9a4a',
    inlays: 'none',
    inlay: '#ffffff',
    nut: '#1a1a1a',
  },
  {
    id: 'or',
    name: t('Or massif'),
    blurb: t('Touche noire, frettes et blocs en or : pour les têtes d’affiche.'),
    price: 1300,
    board: ['#0e0c0a', '#1c1814'],
    pattern: 'grain',
    frets: '#f0c050',
    posts: '#ffcf5a',
    inlays: 'blocks',
    inlay: '#ffd36a',
    glow: true,
    nut: '#f6d77a',
  },
  {
    id: 'neon',
    name: t('Néon'),
    blurb: t('Des filets de lumière le long du manche, qui battent la mesure.'),
    price: 1800,
    board: ['#06060c', '#0c0c18'],
    pattern: 'neon',
    paint: '#2af0ff',
    frets: '#ff3ad8',
    posts: '#ff5ae0',
    inlays: 'dots',
    inlay: '#2af0ff',
    glow: true,
    nut: '#2af0ff',
  },
  {
    id: 'galaxie',
    name: t('Galaxie'),
    blurb: t('Une nuit étoilée sous les cordes, des étoiles pour repères.'),
    price: 2500,
    board: ['#0a0620', '#2a1050'],
    pattern: 'stars',
    paint: '#ffffff',
    frets: '#c8b8ff',
    posts: '#b49aff',
    inlays: 'stars',
    inlay: '#fff0a0',
    glow: true,
    nut: '#d8ccff',
  },
  {
    id: 'relique',
    name: t('Relique'),
    blurb: t('Un palissandre usé jusqu’au bois clair par des centaines de concerts.'),
    price: 0,
    trophy: 'pro',
    board: ['#3a2618', '#56392a'],
    pattern: 'worn',
    paint: '#a07a52',
    frets: '#9a8a70',
    posts: '#c0a882',
    inlays: 'dots',
    inlay: '#d8cbb0',
    nut: '#d8c8a0',
  },
  {
    id: 'lave',
    name: t('Lave'),
    blurb: t('De la roche noire fendue de lave qui pulse avec la salle.'),
    price: 0,
    trophy: 'en-feu',
    board: ['#0a0808', '#161010'],
    pattern: 'lava',
    paint: '#ff5a10',
    frets: '#3a3434',
    posts: '#ff7a2a',
    inlays: 'none',
    inlay: '#ffffff',
    nut: '#ff5a10',
  },
  {
    id: 'platine',
    name: t('Platine'),
    blurb: t('Nacre blanche, chrome et diamants de glace : la guitare des solos parfaits.'),
    price: 0,
    trophy: 'solo',
    board: ['#8c929c', '#aeb4be'],
    pattern: 'grain',
    frets: '#f0f4fa',
    posts: '#d8e8ff',
    inlays: 'diamonds',
    inlay: '#7ac8ff',
    glow: true,
    nut: '#ffffff',
  },
];

/** The guitars the shop sells, cheapest first (the others are won). */
export const FOR_SALE = GUITARS.filter((g) => !g.trophy && g.price > 0);

export const isGuitar = (id: unknown): id is string => GUITARS.some((g) => g.id === id);

/** The guitar `id`, or the one every player has. */
export const guitarOf = (id: string): Guitar => GUITARS.find((g) => g.id === id) ?? GUITARS[0];

/** A picture of the guitar for the shop: headstock, nut and the first frets, strings across. */
export function drawGuitar(canvas: HTMLCanvasElement, g: Guitar): void {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const { width: w, height: h } = canvas;
  const top = h * 0.24;
  const bottom = h * 0.76;
  const nut = w * 0.16;
  const [dark, light] = g.board;
  ctx.clearRect(0, 0, w, h);

  // The headstock, its tuners, the board.
  const head = ctx.createLinearGradient(0, top, 0, bottom);
  head.addColorStop(0, light);
  head.addColorStop(1, dark);
  ctx.fillStyle = head;
  ctx.beginPath();
  ctx.roundRect(w * 0.01, h * 0.1, nut, h * 0.8, h * 0.14);
  ctx.fill();
  ctx.fillStyle = '#c8ccd4';
  for (let i = 0; i < 3; i++) {
    for (const y of [h * 0.17, h * 0.83]) {
      ctx.beginPath();
      ctx.arc(w * 0.035 + i * nut * 0.33, y, h * 0.045, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  ctx.save();
  ctx.beginPath();
  ctx.rect(nut, top, w - nut, bottom - top);
  ctx.clip();
  drawBoard(ctx, g, nut, top, w, bottom);
  ctx.restore();

  // Nut and frets, closer together up the neck.
  ctx.fillStyle = g.nut;
  ctx.fillRect(nut - h * 0.04, top, h * 0.06, bottom - top);
  const frets: number[] = [];
  let x = nut;
  let gap = (w - nut) / 5.2;
  while (x < w) {
    x += gap;
    gap *= 0.94;
    frets.push(x);
  }
  ctx.fillStyle = g.frets;
  for (const f of frets) ctx.fillRect(f - 1.5, top, 3, bottom - top);

  // Inlays between frets 2-3 and 4-5.
  ctx.fillStyle = g.inlay;
  if (g.glow) {
    ctx.shadowColor = g.inlay;
    ctx.shadowBlur = h * 0.12;
  }
  for (const k of [2, 4]) {
    if (k >= frets.length) break;
    inlay(ctx, g.inlays, (frets[k - 1] + frets[k]) / 2, (top + bottom) / 2, Math.min(frets[k] - frets[k - 1], bottom - top));
  }
  ctx.shadowBlur = 0;

  // Strings, from the tuners over the nut.
  ctx.strokeStyle = 'rgba(225, 228, 236, 0.75)';
  for (let s = 0; s < 6; s++) {
    const y = top + ((s + 0.5) / 6) * (bottom - top);
    ctx.lineWidth = 1 + (5 - s) * 0.35;
    ctx.beginPath();
    ctx.moveTo(w * 0.035 + (s % 3) * nut * 0.33, s < 3 ? h * 0.17 : h * 0.83);
    ctx.lineTo(nut, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }
}

function drawBoard(ctx: CanvasRenderingContext2D, g: Guitar, x0: number, top: number, x1: number, bottom: number): void {
  const [dark, light] = g.board;
  const h = bottom - top;
  const paint = g.paint ?? light;
  if (g.pattern === 'burst') {
    const burst = ctx.createLinearGradient(0, top, 0, bottom);
    burst.addColorStop(0, dark);
    burst.addColorStop(0.5, light);
    burst.addColorStop(1, dark);
    ctx.fillStyle = burst;
  } else if (g.pattern === 'stars') {
    const sky = ctx.createLinearGradient(x0, top, x1, bottom);
    sky.addColorStop(0, dark);
    sky.addColorStop(1, light);
    ctx.fillStyle = sky;
  } else ctx.fillStyle = dark;
  ctx.fillRect(x0, top, x1 - x0, h);

  if (g.pattern === 'grain' || g.pattern === 'burst' || g.pattern === 'worn') {
    ctx.strokeStyle = light;
    ctx.globalAlpha = 0.35;
    for (let i = 0; i < 14; i++) {
      const y = top + (i / 14) * h + 2;
      ctx.lineWidth = 1 + (i % 3);
      ctx.beginPath();
      ctx.moveTo(x0, y);
      ctx.bezierCurveTo(x0 + (x1 - x0) * 0.3, y + 3, x0 + (x1 - x0) * 0.6, y - 3, x1, y + 1);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    if (g.pattern === 'worn') {
      // Bare wood where the fingers play, the first frets.
      ctx.fillStyle = paint;
      let seed = 3;
      const rand = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
      for (let i = 0; i < 26; i++) {
        ctx.globalAlpha = 0.25 + rand() * 0.35;
        ctx.beginPath();
        ctx.ellipse(x0 + rand() * (x1 - x0) * 0.55, top + h * (0.2 + rand() * 0.6), 4 + rand() * 12, 2 + rand() * 5, 0, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    }
  } else if (g.pattern === 'lava') {
    ctx.strokeStyle = paint;
    ctx.shadowColor = paint;
    ctx.shadowBlur = h * 0.2;
    ctx.lineWidth = 2;
    for (let k = 0; k < 5; k++) {
      ctx.beginPath();
      for (let x = x0; x <= x1; x += 6) {
        const y = top + h * (0.15 + k * 0.18) + Math.sin(x * 0.05 + k * 2) * h * 0.08 + Math.sin(x * 0.17 + k) * h * 0.03;
        if (x === x0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
  } else if (g.pattern === 'flames') {
    for (let k = 0; k < 4; k++) {
      const mid = top + ((k + 0.5) / 4) * h;
      const reach = x0 + (x1 - x0) * (0.45 + 0.13 * ((k * 7) % 4));
      const fire = ctx.createLinearGradient(x0, 0, reach, 0);
      fire.addColorStop(0, '#ffe27a');
      fire.addColorStop(0.45, paint);
      fire.addColorStop(1, '#b0180a');
      ctx.fillStyle = fire;
      ctx.beginPath();
      ctx.moveTo(x0, mid - h * 0.13);
      ctx.bezierCurveTo((x0 + reach) / 2, mid - h * 0.2, (x0 + reach) / 2, mid + h * 0.08, reach, mid);
      ctx.bezierCurveTo((x0 + reach) / 2, mid + h * 0.22, (x0 + reach) / 2, mid + h * 0.02, x0, mid + h * 0.13);
      ctx.fill();
    }
  } else if (g.pattern === 'neon') {
    ctx.strokeStyle = paint;
    ctx.shadowColor = paint;
    ctx.shadowBlur = h * 0.25;
    ctx.lineWidth = 3;
    for (const y of [top + 4, bottom - 4]) {
      ctx.beginPath();
      ctx.moveTo(x0, y);
      ctx.lineTo(x1, y);
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
  } else if (g.pattern === 'stars') {
    ctx.fillStyle = paint;
    let seed = 7;
    const rand = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
    for (let i = 0; i < 70; i++) {
      const r = rand() < 0.15 ? 1.8 : 0.9;
      ctx.globalAlpha = 0.5 + rand() * 0.5;
      ctx.beginPath();
      ctx.arc(x0 + rand() * (x1 - x0), top + rand() * h, r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }
}

function inlay(ctx: CanvasRenderingContext2D, shape: InlayShape, x: number, y: number, size: number): void {
  ctx.beginPath();
  if (shape === 'dots') ctx.arc(x, y, size * 0.13, 0, Math.PI * 2);
  else if (shape === 'blocks') ctx.rect(x - size * 0.28, y - size * 0.3, size * 0.56, size * 0.6);
  else if (shape === 'sharks') {
    ctx.moveTo(x - size * 0.3, y + size * 0.32);
    ctx.lineTo(x + size * 0.3, y + size * 0.32);
    ctx.lineTo(x + size * 0.3, y - size * 0.32);
    ctx.closePath();
  } else if (shape === 'diamonds') {
    ctx.moveTo(x, y - size * 0.3);
    ctx.lineTo(x + size * 0.2, y);
    ctx.lineTo(x, y + size * 0.3);
    ctx.lineTo(x - size * 0.2, y);
    ctx.closePath();
  } else if (shape === 'stars') {
    for (let i = 0; i < 10; i++) {
      const r = (i % 2 ? 0.1 : 0.24) * size;
      const a = -Math.PI / 2 + (i * Math.PI) / 5;
      ctx.lineTo(x + Math.cos(a) * r, y + Math.sin(a) * r);
    }
    ctx.closePath();
  } else return;
  ctx.fill();
}
