"""Tempo, beats and bar lines, extended to cover the whole song.

Beats: librosa's dynamic-programming tracker on a fine onset envelope (5.8 ms frames, mean over
bands so a kick alone still counts), each beat moved onto its onset peak (sub-frame), smoothed
by a local linear fit so frame quantization does not bias the tempo, and shifted back by the
envelope's measured lag: the chart's beat lines sit on the attacks the player hears.

Tempo: the pulse is chosen by metrical consistency — a 4/4 beat period L shows up in the onset
autocorrelation at L/2, L, 2L and 4L — then by a log-normal prior around 100 BPM to pick the
beat among T/2, T and 2T. Without it, a syncopated riff (3+3+2 eighths, as in a rock song
at 80/160 BPM) pulls the tracker onto its accents, 2/3 of the true tempo.

Bar lines (4/4): the beat phase that best explains several cues — kick/bass attacks, harmonic
changes, long guitar notes — plus the prior that a song's first event falls on a downbeat.
Learned downbeat trackers do better on real music; this keeps the pipeline self-contained.
"""

from __future__ import annotations

import numpy as np

from .builder import BeatGrid
from .fingering import Note

HOP = 128
# The onset envelope peaks 7.5-10.5 ms after an attack (STFT window and flux lag), whatever the
# tempo: measured on click tracks of kicks, snares, clicks and hi-hats at 80-170 BPM.
ANALYSIS_LAG = 0.0085


def _refine(beats: np.ndarray, onset: np.ndarray, sr: int, reach: float = 0.03) -> np.ndarray:
    """Moves each beat onto the onset-envelope peak within `reach` seconds (parabolic peak)."""
    frame = HOP / sr
    out = beats.copy()
    r = int(reach / frame)
    for k, t in enumerate(beats):
        i = int(round(t / frame))
        lo, hi = max(i - r, 1), min(i + r + 1, len(onset) - 1)
        if hi - lo < 3:
            continue
        j = lo + int(np.argmax(onset[lo:hi]))
        a, b, c = onset[j - 1], onset[j], onset[j + 1]
        denom = a - 2 * b + c
        delta = 0.5 * (a - c) / denom if denom < 0 else 0.0
        out[k] = (j + float(np.clip(delta, -0.5, 0.5))) * frame
    return out


def _smooth(beats: np.ndarray, half: int = 4) -> np.ndarray:
    """Local linear fit over ±`half` beats: removes jitter, follows tempo changes."""
    idx = np.arange(len(beats))
    out = beats.copy()
    for k in range(len(beats)):
        sl = slice(max(k - half, 0), min(k + half + 1, len(beats)))
        slope, intercept = np.polyfit(idx[sl], beats[sl], 1)
        out[k] = slope * k + intercept
    return out


def choose_tempo(onset: np.ndarray, sr: int, lo: float = 50.0, hi: float = 200.0) -> float:
    """Tempo (BPM) whose duple metrical hierarchy best matches the onset envelope."""
    import librosa

    frame = HOP / sr
    ac = librosa.autocorrelate(onset - onset.mean(), max_size=int(8.5 / frame))
    if ac[0] <= 0:
        return 120.0
    ac = ac / ac[0]
    reach = max(1, int(0.02 / frame))

    def at(lag: float) -> float:
        i = int(round(lag / frame))
        seg = ac[max(i - reach, 0) : i + reach + 1]
        return float(seg.max()) if len(seg) else 0.0

    best, best_score = 120.0, -np.inf
    for bpm in np.arange(lo, hi, 0.25):
        period = 60.0 / bpm
        salience = at(period) + at(2 * period) + at(4 * period) + 0.5 * at(period / 2)
        score = salience * np.exp(-0.5 * np.log2(bpm / 100.0) ** 2)
        if score > best_score:
            best, best_score = float(bpm), score
    return best


def _zscore(x: np.ndarray) -> np.ndarray:
    s = x.std()
    return (x - x.mean()) / s if s > 1e-12 else np.zeros_like(x)


def track(
    mix: np.ndarray,
    sr: int,
    duration: float,
    rhythm: np.ndarray | None = None,
    notes: list[Note] | None = None,
    beats_per_bar: int = 4,
) -> BeatGrid:
    """Beat grid of a mono `mix`; `rhythm` (drums + bass stems) and transcribed `notes` help."""
    import librosa

    source = rhythm if rhythm is not None else mix
    onset = librosa.onset.onset_strength(y=source, sr=sr, hop_length=HOP)
    tempo = choose_tempo(onset, sr)
    # High tightness: records are played to a pulse; with the default, dense syncopated attacks
    # pull beats off it (a test song drifted from 80 to 87 BPM locally).
    _, frames = librosa.beat.beat_track(
        onset_envelope=onset, sr=sr, hop_length=HOP, bpm=tempo, tightness=400, units="frames"
    )
    if len(frames) < 4:
        return BeatGrid.regular(120.0, duration, 0.0, beats_per_bar)
    beats = _smooth(_refine(librosa.frames_to_time(frames, sr=sr, hop_length=HOP), onset, sr)) - ANALYSIS_LAG
    period = float(np.mean(np.diff(beats)))
    head = np.arange(beats[0] - period, -1e-9, -period)[::-1]
    tail = np.arange(beats[-1] + period, duration + period, period)
    beats = np.concatenate([head, beats, tail])

    # Cues sampled on every beat.
    frame = HOP / sr
    idx = np.clip(np.round((beats + ANALYSIS_LAG) / frame).astype(int), 0, len(onset) - 1)
    spec = np.abs(librosa.stft(source, n_fft=4096, hop_length=HOP))
    low = np.log1p(spec[librosa.fft_frequencies(sr=sr, n_fft=4096) < 150].sum(axis=0))
    flux = np.maximum(np.diff(low, prepend=low[:1]), 0.0)
    kick = np.array([flux[max(i - 2, 0) : i + 3].max() for i in idx])

    chroma = librosa.feature.chroma_stft(y=mix, sr=sr, hop_length=HOP)
    seg = [chroma[:, idx[k] : max(idx[k] + 1, idx[k + 1] if k + 1 < len(idx) else idx[k] + 1)].mean(axis=1) for k in range(len(idx))]
    change = np.zeros(len(seg))
    for k in range(1, len(seg)):
        a, b = seg[k - 1], seg[k]
        change[k] = 1.0 - float(a @ b / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9))

    weight = np.zeros(len(beats))
    first_event = None
    if notes:
        for n in notes:
            k = int(np.argmin(np.abs(beats - n.time)))
            if abs(beats[k] - n.time) < 0.25 * period:
                weight[k] += min(n.duration / period, 2.0)
        first_event = min(n.time for n in notes)
    energy = np.convolve(onset, np.ones(8) / 8, mode="same")
    loud = np.flatnonzero(energy > 0.2 * energy.max())
    if loud.size:
        t_first = loud[0] * frame
        first_event = t_first if first_event is None else min(first_event, t_first)

    cues = _zscore(kick) + _zscore(change) + _zscore(weight)
    scores = np.array([cues[p::beats_per_bar].mean() for p in range(beats_per_bar)])
    if first_event is not None:
        k = int(np.argmin(np.abs(beats - first_event)))
        if abs(beats[k] - first_event) < 0.3 * period:
            scores[k % beats_per_bar] += 1.0
    phase = int(np.argmax(scores))
    return BeatGrid([float(b) for b in beats], list(range(phase, len(beats), beats_per_bar)), beats_per_bar)
