/**
 * The game's sounds, made here rather than recorded: the interface's clicks, the cues in play
 * (count-in, streaks, the grade, a fanfare) and a crowd (its murmur, cheers, applause,
 * clapping in time). Each is mono samples at its own rate, the same on every run (seeded
 * noise), so they can be checked by tests and need no file nor licence.
 */

export const SOUND_NAMES = [
  'hover',
  'click',
  'back',
  'tick',
  'error',
  'count',
  'countAccent',
  'streak',
  'levelUp',
  'grade',
  'fanfare',
  'crowd',
  'cheer',
  'applause',
  'clapping',
] as const;
export type SoundName = (typeof SOUND_NAMES)[number];

/** Sounds that loop (the crowd's background). */
export const LOOPS: readonly SoundName[] = ['crowd', 'clapping'];

const TAU = 2 * Math.PI;

/** Deterministic noise in [-1, 1) (mulberry32). */
function noise(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (((t ^ (t >>> 14)) >>> 0) / 4294967296) * 2 - 1;
  };
}

/** A biquad band-pass (constant peak gain), RBJ's cookbook. */
function bandpass(rate: number, freq: number, q: number): (x: number) => number {
  const w = (TAU * Math.min(freq, rate * 0.45)) / rate;
  const alpha = Math.sin(w) / (2 * q);
  const a0 = 1 + alpha;
  const b0 = alpha / a0;
  const a1 = (-2 * Math.cos(w)) / a0;
  const a2 = (1 - alpha) / a0;
  let x1 = 0;
  let x2 = 0;
  let y1 = 0;
  let y2 = 0;
  return (x) => {
    const y = b0 * x - b0 * x2 - a1 * y1 - a2 * y2;
    x2 = x1;
    x1 = x;
    y2 = y1;
    y1 = y;
    return y;
  };
}

/** A one-pole low-pass at `freq`. */
function lowpass(rate: number, freq: number): (x: number) => number {
  const k = 1 - Math.exp((-TAU * freq) / rate);
  let y = 0;
  return (x) => (y += k * (x - y));
}

/** Rises in `attack` s, then decays with time constant `tau`. */
const env = (t: number, attack: number, tau: number): number => (t < 0 ? 0 : t < attack ? t / attack : Math.exp(-(t - attack) / tau));

/** Band-limited sawtooth of `harmonics` partials at phase `ph` (cycles). */
const saw = (ph: number, harmonics: number): number => {
  let v = 0;
  for (let k = 1; k <= harmonics; k++) v += Math.sin(TAU * k * ph) / k;
  return v * 0.55;
};

function buffer(rate: number, seconds: number): Float32Array {
  return new Float32Array(Math.round(rate * seconds));
}

/** Scaled so that its loudest sample is `peak`. */
function normalize(x: Float32Array, peak: number): Float32Array {
  let max = 0;
  for (const v of x) max = Math.max(max, Math.abs(v));
  if (max > 0) for (let i = 0; i < x.length; i++) x[i] *= peak / max;
  return x;
}

/** A tone gliding from `from` to `to` Hz over `glide` s, with an envelope. */
function blip(rate: number, seconds: number, from: number, to: number, glide: number, attack: number, tau: number, peak: number): Float32Array {
  const x = buffer(rate, seconds);
  let ph = 0;
  for (let i = 0; i < x.length; i++) {
    const t = i / rate;
    const f = t < glide ? from + ((to - from) * t) / glide : to;
    ph += f / rate;
    x[i] = Math.sin(TAU * ph) * env(t, attack, tau);
  }
  return normalize(x, peak);
}

/** A struck bell at `freq`: its partials (1, 2.76, 5.4) dying away at their own pace. */
function bell(x: Float32Array, rate: number, at: number, freq: number, gain: number, tau = 0.35): void {
  const start = Math.round(at * rate);
  for (let i = start; i < x.length; i++) {
    const t = (i - start) / rate;
    const e = env(t, 0.004, tau);
    if (e < 1e-4 && t > 0.01) break;
    x[i] += gain * e * (Math.sin(TAU * freq * t) + 0.35 * Math.sin(TAU * 2.76 * freq * t) * Math.exp(-t / (tau * 0.4)) + 0.15 * Math.sin(TAU * 5.4 * freq * t) * Math.exp(-t / (tau * 0.2)));
  }
}

/** One clap: a burst of noise a few milliseconds long, through the hands' resonance. */
function clap(x: Float32Array, rate: number, at: number, freq: number, gain: number, rand: () => number): void {
  const filter = bandpass(rate, freq, 1.6);
  const start = Math.round(at * rate);
  const length = Math.round((0.012 + 0.01 * Math.abs(rand())) * rate);
  for (let i = 0; i < length && start + i < x.length; i++) {
    const t = i / rate;
    x[start + i] += gain * filter(rand()) * env(t, 0.0008, 0.0035);
  }
}

/** Many people talking: noise through vowel formants that change at the pace of syllables. */
function babble(x: Float32Array, rate: number, talkers: number, gain: number, rand: () => number): void {
  const VOWELS = [
    [730, 1090],
    [530, 1840],
    [570, 840],
    [300, 870],
    [440, 1020],
    [270, 2290],
  ];
  for (let k = 0; k < talkers; k++) {
    let f1 = bandpass(rate, 500, 6);
    let f2 = bandpass(rate, 1500, 8);
    let next = 0;
    let syllable = 0.2;
    let level = 0;
    const voice = 0.6 + 0.4 * Math.abs(rand());
    for (let i = 0; i < x.length; i++) {
      const t = i / rate;
      if (t >= next) {
        const [a, b] = VOWELS[Math.floor(Math.abs(rand()) * VOWELS.length) % VOWELS.length];
        const shift = 0.85 + 0.3 * Math.abs(rand());
        f1 = bandpass(rate, a * shift, 6);
        f2 = bandpass(rate, b * shift, 8);
        syllable = 0.12 + 0.2 * Math.abs(rand());
        // Now and then a talker pauses.
        level = rand() > -0.6 ? voice : 0;
        next = t + syllable;
      }
      const into = (t - (next - syllable)) / syllable;
      const shape = Math.sin(Math.PI * Math.min(1, Math.max(0, into)));
      const n = rand();
      x[i] += gain * level * shape * (f1(n) + 0.6 * f2(n));
    }
  }
}

/** "Wooo": voices sliding up and holding, through an "oo" to "oh" vowel. */
function woo(x: Float32Array, rate: number, voices: number, from: number, length: number, gain: number, rand: () => number): void {
  for (let k = 0; k < voices; k++) {
    const f0 = 220 + 200 * Math.abs(rand());
    const start = from + 0.4 * Math.abs(rand());
    const hold = length * (0.6 + 0.4 * Math.abs(rand()));
    const vib = 5 + 1.5 * rand();
    const f1 = bandpass(rate, 380 + 120 * Math.abs(rand()), 5);
    const f2 = bandpass(rate, 850 + 150 * Math.abs(rand()), 6);
    let ph = 0;
    const s0 = Math.round(start * rate);
    const s1 = Math.min(x.length, Math.round((start + hold) * rate));
    for (let i = s0; i < s1; i++) {
      const t = (i - s0) / rate;
      const rise = Math.min(1, t / 0.35);
      const f = f0 * (1 + 0.35 * rise * rise) * (1 + 0.012 * Math.sin(TAU * vib * t)) * (t > hold - 0.4 ? 1 - 0.25 * ((t - hold + 0.4) / 0.4) : 1);
      ph += f / rate;
      const e = Math.min(1, t / 0.15) * Math.min(1, (hold - t) / 0.3);
      const v = saw(ph, 12);
      x[i] += gain * e * (f1(v) + 0.7 * f2(v));
    }
  }
}

/** The last `fade` seconds blended into the first ones: the sound loops without a seam. */
function loopable(x: Float32Array, rate: number, fade: number): Float32Array {
  const n = Math.round(fade * rate);
  const out = x.slice(0, x.length - n);
  for (let i = 0; i < n; i++) {
    const k = i / n;
    out[i] = out[i] * Math.sqrt(k) + x[x.length - n + i] * Math.sqrt(1 - k);
  }
  return out;
}

/** Applause by `people` clapping at their own pace, rising fast and dying away over `seconds`. */
function applause(rate: number, seconds: number, people: number, seed: number): Float32Array {
  const x = buffer(rate, seconds);
  const rand = noise(seed);
  for (let p = 0; p < people; p++) {
    const pace = 3.2 + 1.8 * Math.abs(rand());
    const hand = 900 + 1600 * Math.abs(rand());
    const loud = 0.5 + 0.5 * Math.abs(rand());
    // Each stops at their own time in the second half.
    const stop = seconds * (0.45 + 0.5 * Math.abs(rand()));
    for (let t = 0.05 * Math.abs(rand()); t < stop; t += (1 + 0.15 * rand()) / pace) {
      const fade = t > stop - 0.6 ? (stop - t) / 0.6 : 1;
      clap(x, rate, t, hand * (1 + 0.08 * rand()), loud * fade, rand);
    }
  }
  return x;
}

/** The sound `name`, mono at `rate` Hz. */
export function synthesize(name: SoundName, rate: number): Float32Array {
  switch (name) {
    case 'hover':
      return blip(rate, 0.03, 2600, 2600, 0, 0.001, 0.005, 0.12);
    case 'click': {
      const x = blip(rate, 0.09, 900, 1350, 0.03, 0.002, 0.022, 0.4);
      const rand = noise(7);
      const hp = lowpass(rate, 3000);
      for (let i = 0; i < 0.004 * rate; i++) {
        const n = rand();
        x[i] += 0.15 * (n - hp(n)) * (1 - i / (0.004 * rate));
      }
      return x;
    }
    case 'back':
      return blip(rate, 0.1, 1250, 760, 0.04, 0.002, 0.025, 0.35);
    case 'tick': {
      const x = buffer(rate, 0.03);
      const rand = noise(11);
      const bp = bandpass(rate, 3200, 3);
      for (let i = 0; i < x.length; i++) {
        const t = i / rate;
        x[i] = bp(rand()) * env(t, 0.0005, 0.004) + 0.4 * Math.sin(TAU * 1800 * t) * env(t, 0.0005, 0.006);
      }
      return normalize(x, 0.18);
    }
    case 'error': {
      const x = buffer(rate, 0.22);
      const lp = lowpass(rate, 1400);
      for (let i = 0; i < x.length; i++) {
        const t = i / rate;
        let v = 0;
        for (const f of [196, 207]) for (let k = 1; k <= 7; k += 2) v += Math.sin(TAU * k * f * t) / k;
        x[i] = lp(v) * env(t, 0.004, 0.07);
      }
      return normalize(x, 0.35);
    }
    case 'count':
    case 'countAccent': {
      const f = name === 'count' ? 1000 : 1500;
      const x = buffer(rate, 0.08);
      for (let i = 0; i < x.length; i++) {
        const t = i / rate;
        x[i] = (Math.sin(TAU * f * t) + 0.4 * Math.sin(TAU * 2.7 * f * t) * Math.exp(-t / 0.006)) * env(t, 0.0008, 0.016);
      }
      return normalize(x, 0.5);
    }
    case 'streak': {
      const x = buffer(rate, 0.75);
      [784, 988, 1175, 1568].forEach((f, k) => bell(x, rate, k * 0.055, f, 1 - k * 0.12, 0.28));
      return normalize(x, 0.32);
    }
    case 'levelUp': {
      const x = buffer(rate, 0.6);
      bell(x, rate, 0, 1175, 1, 0.25);
      bell(x, rate, 0.09, 1760, 0.9, 0.3);
      return normalize(x, 0.28);
    }
    case 'grade': {
      const x = buffer(rate, 0.9);
      const rand = noise(23);
      const lp = lowpass(rate, 1500);
      let ph = 0;
      for (let i = 0; i < x.length; i++) {
        const t = i / rate;
        ph += (45 + 95 * Math.exp(-t / 0.08)) / rate;
        x[i] = Math.sin(TAU * ph) * env(t, 0.003, 0.25) + 0.5 * lp(rand()) * env(t, 0.001, 0.04);
      }
      bell(x, rate, 0.02, 2093, 0.12, 0.4);
      return normalize(x, 0.6);
    }
    case 'fanfare': {
      const x = buffer(rate, 1.5);
      const notes: [number, number, number][] = [
        [0, 523.25, 0.12],
        [0.12, 659.25, 0.12],
        [0.24, 783.99, 0.12],
        [0.36, 1046.5, 0.9],
      ];
      for (const [at, f, hold] of notes) {
        const lp = lowpass(rate, 2600);
        let ph = 0;
        const s0 = Math.round(at * rate);
        const s1 = Math.min(x.length, Math.round((at + hold + 0.25) * rate));
        for (let i = s0; i < s1; i++) {
          const t = (i - s0) / rate;
          ph += (f * (1 + (t > 0.15 ? 0.006 * Math.sin(TAU * 5.5 * t) : 0))) / rate;
          const e = Math.min(1, t / 0.015) * (t < hold ? 1 : Math.exp(-(t - hold) / 0.08));
          x[i] += lp(saw(ph, 10)) * e;
        }
      }
      return normalize(x, 0.4);
    }
    case 'crowd': {
      // A room full: talkers at different distances, a low rumble, now and then a shout.
      const x = buffer(rate, 8.6);
      const rand = noise(31);
      babble(x, rate, 18, 1, rand);
      const lp = lowpass(rate, 180);
      for (let i = 0; i < x.length; i++) x[i] += 0.6 * lp(rand());
      for (const at of [1.3, 3.9, 6.2]) woo(x, rate, 1, at, 0.7, 0.35, rand);
      return normalize(loopable(x, rate, 0.6), 0.55);
    }
    case 'cheer': {
      const x = buffer(rate, 3.6);
      const rand = noise(41);
      babble(x, rate, 16, 0.8, rand);
      woo(x, rate, 14, 0.05, 2.6, 1, rand);
      const claps = applause(rate, 3.6, 24, 43);
      for (let i = 0; i < x.length; i++) {
        const t = i / rate;
        x[i] = x[i] * Math.min(1, t / 0.25) * Math.min(1, (3.6 - t) / 1.2) + 0.8 * claps[i];
      }
      return normalize(x, 0.7);
    }
    case 'applause':
      return normalize(applause(rate, 5, 36, 53), 0.6);
    case 'clapping': {
      // Clapping in time, two claps a second, as a crowd calls for one more song.
      // Made half a second longer, the end blended into the start: four seconds that loop.
      const x = buffer(rate, 4.5);
      const rand = noise(61);
      babble(x, rate, 10, 0.25, rand);
      for (let p = 0; p < 30; p++) {
        const hand = 900 + 1600 * Math.abs(rand());
        const loud = 0.5 + 0.5 * Math.abs(rand());
        for (let beat = 0; beat < 9; beat++) clap(x, rate, beat / 2 + 0.012 * (rand() + 1), hand, loud, rand);
      }
      return normalize(loopable(x, rate, 0.5), 0.6);
    }
  }
}

/** Rate each sound is made at: the crowd needs less. */
export function rateOf(name: SoundName): number {
  return ['crowd', 'cheer', 'applause', 'clapping'].includes(name) ? 24_000 : 48_000;
}
