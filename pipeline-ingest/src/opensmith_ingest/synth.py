"""Synthetic band (overdriven guitar, bass, rock drums) with exact ground truth.

Used by the end-to-end test and as a demo song: every guitar note, beat and bar is known. The
guitar is a Karplus-Strong string through an amp (pre-emphasis, asymmetric clipping, 4x12-like
cabinet): an additive "pluck" made of decaying sines is heard as a piano by Demucs.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from scipy import signal

from .fingering import Note


def _pluck(freq: float, dur: float, sr: int, rng: np.random.Generator, partials: int = 30, t60: float = 2.0) -> np.ndarray:
    """Additive plucked string: pluck-position comb, stiffness, faster decay of high partials."""
    n = int(dur * sr)
    t = np.arange(n) / sr
    pos = rng.uniform(0.12, 0.3)
    out = np.zeros(n)
    for h in range(1, partials + 1):
        f = freq * h * np.sqrt(1 + 1e-4 * h * h)
        if f > 0.45 * sr:
            break
        amp = abs(np.sin(np.pi * h * pos)) / h
        decay = np.exp(-6.9 * t * (1 + 0.05 * (h - 1) ** 2) / t60)
        out += amp * decay * np.sin(2 * np.pi * f * t + rng.uniform(0, 0.2))
    attack = np.minimum(t / 0.002, 1.0)
    release = np.clip((dur - t) / 0.02, 0.0, 1.0)
    return out * attack * release


def _string(freq: float, dur: float, sr: int, rng: np.random.Generator, t60: float = 3.0) -> np.ndarray:
    """Karplus-Strong string as one IIR comb (y[n] = x[n] + g/2 (y[n-L] + y[n-L-1]))."""
    n = int(dur * sr)
    period = int(round(sr / freq - 0.5))
    g = 10 ** (-3.0 / (t60 * freq))
    excitation = np.zeros(n)
    burst = rng.uniform(-1, 1, period)
    pos = max(1, int(rng.uniform(0.12, 0.3) * period))
    burst = burst - np.roll(burst, pos)  # pluck position
    excitation[:period] = signal.lfilter([0.5, 0.5], [1.0], burst)  # pick brightness
    a = np.zeros(period + 2)
    a[0], a[period], a[period + 1] = 1.0, -g / 2, -g / 2
    y = signal.lfilter([1.0], a, excitation)
    release = np.clip((dur - np.arange(n) / sr) / 0.015, 0.0, 1.0)
    return y / (np.abs(y[: 4 * period]).max() + 1e-9) * release


def _amp(x: np.ndarray, sr: int, gain: float = 12.0) -> np.ndarray:
    """Crunch amp: tighten, clip asymmetrically, 4x12 cabinet (80 Hz - 5 kHz, mid bump)."""
    x = signal.sosfilt(signal.butter(2, 120, "highpass", fs=sr, output="sos"), x)
    y = np.tanh(gain * x + 0.2) - np.tanh(0.2)
    y = signal.sosfilt(signal.butter(2, 80, "highpass", fs=sr, output="sos"), y)
    y = signal.sosfilt(signal.butter(4, 4500, "lowpass", fs=sr, output="sos"), y)
    b, a = signal.iirpeak(1800, 1.2, fs=sr)
    return y + 0.6 * signal.lfilter(b, a, y)


def _kick(sr: int) -> np.ndarray:
    t = np.arange(int(0.25 * sr)) / sr
    f = 50 + 70 * np.exp(-t / 0.03)
    return np.sin(2 * np.pi * np.cumsum(f) / sr) * np.exp(-t / 0.12)


def _snare(sr: int, rng: np.random.Generator) -> np.ndarray:
    t = np.arange(int(0.2 * sr)) / sr
    return (0.7 * rng.standard_normal(t.size) + 0.5 * np.sin(2 * np.pi * 190 * t)) * np.exp(-t / 0.05)


def _hat(sr: int, rng: np.random.Generator) -> np.ndarray:
    t = np.arange(int(0.05 * sr)) / sr
    noise = np.diff(rng.standard_normal(t.size + 1))
    return 0.3 * noise * np.exp(-t / 0.012)


@dataclass
class Song:
    sr: int
    bpm: float
    mix: np.ndarray
    guitar: np.ndarray
    backing: np.ndarray
    guitar_notes: list[Note]
    beats: list[float]
    bars: list[float]
    duration: float = field(init=False)

    def __post_init__(self) -> None:
        self.duration = self.mix.shape[-1] / self.sr


def midi_hz(m: int) -> float:
    return 440.0 * 2 ** ((m - 69) / 12)


def rock_song(sr: int = 44_100, bpm: float = 110.0, bars: int = 16, lead_in: float = 1.0, seed: int = 0) -> Song:
    """Two alternating 2-bar riffs (power chords + single-note fills) over bass and drums."""
    rng = np.random.default_rng(seed)
    beat = 60.0 / bpm
    total = lead_in + bars * 4 * beat + 2.0
    n = int(total * sr)
    guitar, bass, drums = np.zeros(n), np.zeros(n), np.zeros(n)
    take2 = np.zeros(n)
    notes: list[Note] = []

    def add(buf: np.ndarray, sig: np.ndarray, t: float, gain: float) -> None:
        i = int(round(t * sr))
        j = min(i + sig.size, n)
        buf[i:j] += gain * sig[: j - i]

    # Riff A: E5 (beat 1, held) - G5 - A5 | single notes;  Riff B: C5 - D5 | fill.
    riff_a = [(0.0, [40, 47, 52], 1.0), (1.5, [43, 50, 55], 0.5), (2.0, [45, 52, 57], 1.5),
              (4.0, [52], 0.5), (4.5, [55], 0.5), (5.0, [57], 0.5), (5.5, [55], 0.5), (6.0, [52], 0.75), (7.0, [50], 0.5)]
    riff_b = [(0.0, [48, 55, 60], 1.5), (2.0, [50, 57, 62], 1.5),
              (4.0, [57], 0.5), (4.5, [59], 0.5), (5.0, [60], 0.5), (5.5, [62], 0.5), (6.0, [64], 1.0), (7.0, [62], 0.5)]
    roots = {40: 28, 43: 31, 45: 33, 48: 36, 50: 38}
    for bar in range(0, bars, 2):
        riff = riff_a if (bar // 2) % 4 != 3 else riff_b
        t_bar = lead_in + bar * 4 * beat
        for at, pitches, length in riff:
            t = t_bar + at * beat
            dur = length * beat
            for p in pitches:
                add(guitar, _string(midi_hz(p), dur, sr, rng), t, 0.3 / np.sqrt(len(pitches)))
                add(take2, _string(midi_hz(p) * 1.0015, dur, sr, rng), t + rng.uniform(0.004, 0.012), 0.3 / np.sqrt(len(pitches)))
                notes.append(Note(t, p, dur))
        # Bass: eighth notes on the root of the last power chord.
        chord_roots = [(at, pitches[0]) for at, pitches, _ in riff if len(pitches) > 1]
        for b8 in range(16):
            t = t_bar + b8 * 0.5 * beat
            current = [r for at, r in chord_roots if at <= b8 * 0.5]
            root = roots[current[-1] if current else chord_roots[0][1]]
            add(bass, _pluck(midi_hz(root), 0.45 * beat, sr, rng, partials=12, t60=1.0), t, 0.5)
        for b in range(8):
            t = t_bar + b * beat
            add(drums, _kick(sr) if b % 2 == 0 else _snare(sr, rng), t, 0.6 if b % 2 == 0 else 0.35)
            for half in (0.0, 0.5):
                add(drums, _hat(sr, rng), t + half * beat, 1.0)

    left, right = _amp(guitar, sr, gain=30.0), _amp(take2, sr, gain=30.0)
    scale = 0.5 / max(np.abs(left).max(), np.abs(right).max())
    left, right = left * scale, right * scale
    backing = np.stack([bass + drums, bass + drums])
    guitar_st = np.stack([0.85 * left + 0.15 * right, 0.15 * left + 0.85 * right])
    mix = guitar_st + backing
    peak = np.abs(mix).max() / 0.9
    beats = [lead_in + k * beat for k in range(bars * 4)]
    f32 = lambda x: (x / peak).astype(np.float32)  # noqa: E731
    return Song(sr, bpm, f32(mix), f32(guitar_st), f32(backing), notes, beats, beats[::4])
