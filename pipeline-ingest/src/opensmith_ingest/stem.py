"""What the isolated guitar itself says: where strings are struck, where it is silent, its pitch.

Basic Pitch hears pitches well but needs checking against the stem it transcribed:

- attacks: onsets found section by section. Their threshold follows the local loudness, so a
  sparse clean intro (big, isolated attacks) does not hide the attacks of a dense, compressed
  distorted chorus, which a threshold set over the whole song does;
- level: a note where the stem is silent is bleed or a hallucination (Basic Pitch normalises
  its input and transcribes the faintest residue);
- pitch: YIN measures the period of the waveform. A low guitar note recorded through an amp
  often has a fundamental 20-30 dB weaker than its octave, and Basic Pitch then writes the
  octave above; the waveform still repeats at the low note's period, so YIN finds it.
"""

from __future__ import annotations

from functools import cached_property
from pathlib import Path

import numpy as np

RATE = 22_050
# Guitar fundamentals: B1 (Drop B low string) to the 24th fret of the high string.
FMIN = 55.0
FMAX = 1400.0
# A note this far below the stem's usual level is not played on the guitar (dB).
SILENCE_DB = 40.0


class GuitarStem:
    """Analyses of a mono guitar stem at `RATE`."""

    def __init__(self, y: np.ndarray, sr: int = RATE):
        if sr != RATE:
            import librosa

            y = librosa.resample(np.asarray(y, dtype=np.float32), orig_sr=sr, target_sr=RATE)
        self.y = np.asarray(y, dtype=np.float32)
        self.sr = RATE

    @classmethod
    def load(cls, path: Path) -> GuitarStem:
        import librosa

        y, _ = librosa.load(str(path), sr=RATE, mono=True)
        return cls(y)

    # Attacks ------------------------------------------------------------------------------

    @cached_property
    def attacks(self) -> np.ndarray:
        """Onset times (s), the peak-picking threshold scaled by the loudness of the last and
        next few seconds."""
        return adaptive_onsets(self.y, self.sr)

    # Level --------------------------------------------------------------------------------

    _LEVEL_HOP = 256

    @cached_property
    def _level(self) -> np.ndarray:
        import librosa

        rms = librosa.feature.rms(y=self.y, frame_length=1024, hop_length=self._LEVEL_HOP, center=False)[0]
        return 20 * np.log10(rms + 1e-9)

    @cached_property
    def reference_db(self) -> float:
        """The stem's usual playing level: 90th percentile of its non-silent frames (dBFS)."""
        sounding = self._level[self._level > -90]
        return float(np.percentile(sounding, 90)) if sounding.size else -90.0

    def level_db(self, t: float, span: float = 0.1) -> float:
        """Loudest frame of `t .. t + span` (dBFS)."""
        a = max(int(t * self.sr / self._LEVEL_HOP), 0)
        b = max(int((t + span) * self.sr / self._LEVEL_HOP), a + 1)
        frames = self._level[a:b]
        return float(frames.max()) if frames.size else -180.0

    def audible(self, t: float) -> bool:
        """Is the guitar heard at `t` at all?"""
        return self.level_db(t) >= self.reference_db - SILENCE_DB

    # Pitch --------------------------------------------------------------------------------

    def pitch(self, times: np.ndarray | list[float]) -> tuple[np.ndarray, np.ndarray]:
        """YIN at each time: (fractional MIDI pitch, aperiodicity 0..1, low = clearly periodic)."""
        f0, ap = yin(self.y, self.sr, np.asarray(times, dtype=float))
        return 69 + 12 * np.log2(f0 / 440.0), ap


def adaptive_onsets(y: np.ndarray, sr: int, hop: int = 128, window: float = 4.0, delta: float = 0.05) -> np.ndarray:
    """Spectral-flux onsets, peak-picked after dividing the flux by its local maximum (over
    `window` seconds, smoothed): each section is judged against its own strongest attacks, as a
    whole song is by a global threshold. The scale has a floor, so near-silence is not blown up
    into onsets."""
    import librosa
    from scipy.ndimage import maximum_filter1d, uniform_filter1d

    env = librosa.onset.onset_strength(y=y, sr=sr, hop_length=hop)
    if not env.size or env.max() <= 0:
        return np.zeros(0)
    size = max(int(window * sr / hop) | 1, 3)
    scale = uniform_filter1d(maximum_filter1d(env, size, mode="nearest"), size, mode="nearest")
    floor = 0.1 * float(np.percentile(env, 98))
    norm = env / np.maximum(scale, floor)
    frames = librosa.util.peak_pick(
        norm,
        pre_max=max(int(0.03 * sr / hop), 1),
        post_max=1,
        pre_avg=max(int(0.1 * sr / hop), 1),
        post_avg=int(0.1 * sr / hop) + 1,
        delta=delta,
        wait=max(int(0.03 * sr / hop), 1),
    )
    return librosa.frames_to_time(frames, sr=sr, hop_length=hop)


def yin(
    y: np.ndarray,
    sr: int,
    times: np.ndarray,
    fmin: float = FMIN,
    fmax: float = FMAX,
    window: int = 1024,
    threshold: float = 0.15,
    chunk: int = 256,
) -> tuple[np.ndarray, np.ndarray]:
    """YIN (de Cheveigné & Kawahara, 2002) on `window` samples from each of `times`: f0 (Hz)
    and the aperiodicity of the chosen period.

    As in YIN, the first dip of the normalised difference below `threshold` is the period. Then
    one check for weak fundamentals: if the signal repeats clearly better at twice that period,
    the note is the octave below (its fundamental was too weak for the first dip to see it)."""
    tmin, tmax = int(sr / fmax), int(sr / fmin) + 1
    n = window + tmax + 2
    size = 1 << int(np.ceil(np.log2(n + window)))
    if len(y) < n:
        y = np.pad(y, (0, n - len(y)))
    starts = np.clip(np.round(times * sr).astype(int), 0, len(y) - n)
    f0 = np.full(len(starts), np.nan)
    aperiodicity = np.ones(len(starts))
    taus = np.arange(tmax + 2)
    for c in range(0, len(starts), chunk):
        idx = starts[c : c + chunk]
        x = y[idx[:, None] + np.arange(n)].astype(np.float64)
        r = np.fft.irfft(np.fft.rfft(x, n=size) * np.conj(np.fft.rfft(x[:, :window], n=size)), n=size)[:, : tmax + 2]
        energy = np.concatenate([np.zeros((len(x), 1)), np.cumsum(x**2, axis=1)], axis=1)
        d = np.maximum(energy[:, window : window + 1] + energy[:, taus + window] - energy[:, taus] - 2 * r, 0.0)
        cmnd = np.ones_like(d)
        cmnd[:, 1:] = d[:, 1:] * taus[1:] / np.maximum(np.cumsum(d[:, 1:], axis=1), 1e-12)
        for i, row in enumerate(cmnd):
            if energy[i, -1] < 1e-10:
                continue
            seg = row[tmin:tmax]
            dips = np.flatnonzero((seg < threshold) & (seg <= row[tmin + 1 : tmax + 1]))
            k = tmin + (int(dips[0]) if dips.size else int(np.argmin(seg)))
            while 2 * k + 4 <= tmax:
                j = 2 * k - 3 + int(np.argmin(row[2 * k - 3 : 2 * k + 4]))
                if row[j] >= 0.5 * row[k]:
                    break
                k = j
            a, b, e = row[k - 1], row[k], row[k + 1]
            den = a - 2 * b + e
            shift = float(np.clip(0.5 * (a - e) / den, -1, 1)) if abs(den) > 1e-12 else 0.0
            f0[c + i] = sr / (k + shift)
            aperiodicity[c + i] = b
    return f0, aperiodicity
