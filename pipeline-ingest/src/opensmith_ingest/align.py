"""A human tab (Guitar Pro) placed on the recording: the tab's notes at the song's times.

The tab knows what is played, not when: it keeps its written tempo while the band drifts, and a
music video may open with a minute the tab does not have. Both sides become chroma (energy of
the 12 pitch classes over time), the tab's from its notes, the recording's from the guitar
stem, and dynamic time warping finds the monotonic mapping of tab time onto song time under
which they match best (the whole tab, somewhere in the song). The mapping is smoothed — a band
does not change tempo within a tenth of a second — then every chord or note moves onto the
attack the stem has where the mapping puts it: a note of the same pitch heard by the
transcription if there is one, else any attack of the guitar.

A tab sits next to its recording under a name that says the same song
("larsen-come_back_home.gp3" for "Larsen - Come Back Home (Official Music Video).mp3").
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from .builder import BeatGrid
from .fingering import Fingered, Note
from .importers.guitarpro import TabPart
from .stem import GuitarStem
from .transcription import RawNote

# DTW frame (samples at the stem's 22 050 Hz): 93 ms.
HOP = 2048
# Warping steps (tab frames, song frames): the song plays the tab between half and twice its
# written speed. Each step weighs the tab frames it covers, so a path costs the same whatever
# stretch of song it spans: without that, squeezing the whole tab into the last minute of the
# song is "cheaper" than playing it from the start.
STEPS = np.array([[1, 1], [1, 2], [2, 1]])
STEP_WEIGHTS = np.array([1.0, 1.0, 2.0])
# Tab seconds on each side over which the mapping is averaged.
SMOOTH = 2.0
# How far from the chroma mapping a transcribed note of the slice's pitch is looked for (s),
# and over how many slices around the local lag is taken.
LAG_SEARCH = 0.4
LAG_SLICES = 9
# Then a transcribed note of one of the slice's pitches this close is the slice (s); failing
# that, the same an octave off or an attack of the stem, this close.
SNAP_PITCH = 0.08
SNAP_ATTACK = 0.05
OCTAVES = (0, 12, -12)

# Below this share of its notes heard, a tab is not this recording's.
MIN_CONFIRMED = 0.15

TAB_SUFFIXES = (".gp3", ".gp4", ".gp5")
# Words in file names that say nothing about the song.
NOISE = {
    "official", "music", "video", "audio", "lyrics", "lyric", "hd", "hq", "4k", "upgrade", "remastered",
    "remaster", "version", "clip", "live", "feat", "ft", "the", "tab", "tabs", "guitar", "pro",
}


@dataclass
class Placement:
    """A tab placed on a recording."""

    tab: list[Fingered]
    grid: BeatGrid
    # Share of the tab's slices that landed on a transcribed note of their pitch.
    confirmed: float
    # Song time of the tab's start and end.
    start: float
    end: float
    # Song seconds per tab second, first note to last.
    stretch: float

    @property
    def plausible(self) -> bool:
        """Does the tab fit the recording? Another song's tab gets almost none of its notes
        heard; the right one a third or more (the transcription misses many)."""
        return self.confirmed >= MIN_CONFIRMED and 0.6 <= self.stretch <= 1.7


def words(name: str) -> set[str]:
    return {w for w in re.split(r"[^0-9a-z]+", name.lower()) if w and w not in NOISE}


def find_tab(audio: Path) -> Path | None:
    """The tab next to `audio` whose name says the same song, if any: all of its words in the
    audio file's name (noise such as "Official Music Video" aside)."""
    audio = Path(audio)
    song = words(audio.stem)
    best, best_score = None, 0.0
    for path in sorted(audio.parent.iterdir()):
        if path.suffix.lower() not in TAB_SUFFIXES or path.name.startswith("."):
            continue
        tab = words(path.stem)
        if len(tab) < 2:
            continue
        score = len(tab & song) / len(tab)
        if score > best_score:
            best, best_score = path, score
    return best if best_score >= 0.99 else None


def tab_chroma(tab: list[Fingered], duration: float, hop: float, decay: float = 1.0) -> np.ndarray:
    """12 × frames: each note's pitch class from its attack, fading over `decay` seconds."""
    n = int(duration / hop) + 2
    out = np.zeros((12, n))
    for sl in tab:
        for note in sl.notes:
            a = int(note.time / hop)
            length = max(int(min(note.duration, 4 * decay) / hop), 1)
            b = min(a + length, n)
            if a < n:
                out[note.pitch % 12, a:b] += np.exp(-np.arange(b - a) * hop / decay)
    return out


def _normalise(c: np.ndarray) -> np.ndarray:
    c = c + 1e-3
    return c / np.linalg.norm(c, axis=0, keepdims=True)


def place(part: TabPart, stem: GuitarStem, raw: list[RawNote] | None = None) -> Placement:
    """`part`'s notes and beats at the song times of the recording whose guitar is `stem`;
    `raw` (the transcription, optional) sharpens each note onto the attack of its own pitch."""
    import librosa

    hop = HOP / stem.sr
    audio = _normalise(librosa.feature.chroma_cqt(y=stem.y, sr=stem.sr, hop_length=HOP))
    tab = _normalise(tab_chroma(part.tab, part.duration, hop))
    _, path = librosa.sequence.dtw(
        X=tab, Y=audio, metric="cosine", subseq=True, step_sizes_sigma=STEPS, weights_mul=STEP_WEIGHTS
    )
    path = path[::-1]
    # Song frame of each tab frame (several when the song lingers), then a moving average.
    song_frame = np.zeros(tab.shape[1])
    counts = np.zeros(tab.shape[1])
    np.add.at(song_frame, path[:, 0], path[:, 1])
    np.add.at(counts, path[:, 0], 1)
    song_frame = np.maximum.accumulate(song_frame / np.maximum(counts, 1))
    width = max(int(SMOOTH / hop), 1)
    # Odd reflection carries the tempo on past both ends (a flat edge would drag them late).
    padded = np.pad(song_frame, width, mode="reflect", reflect_type="odd")
    smooth = np.convolve(padded, np.ones(2 * width + 1) / (2 * width + 1), mode="same")[width:-width]
    # Tab frame k holds attacks from k·hop on: its centre is half a frame later on both sides.
    tab_times = np.arange(tab.shape[1]) * hop

    def to_song(t: float) -> float:
        return float(np.interp(t, tab_times, smooth * hop))

    by_pitch: dict[int, np.ndarray] = {}
    for n in raw or []:
        by_pitch.setdefault(n.pitch, []).append(n.start)  # type: ignore[arg-type]
    by_pitch = {p: np.array(sorted(ts)) for p, ts in by_pitch.items()}
    attacks = np.asarray(stem.attacks)

    def nearest(times: np.ndarray | None, t: float, within: float) -> float | None:
        if times is None or not len(times):
            return None
        k = int(np.searchsorted(times, t))
        best = min((times[i] for i in (k - 1, k) if 0 <= i < len(times)), key=lambda x: abs(x - t))
        return float(best) if abs(best - t) <= within else None

    def heard(sl: Fingered, t: float, within: float, after: float = -np.inf, octaves: tuple[int, ...] = (0,)) -> float | None:
        """Attack of a transcribed note of the slice's pitch (moved by `octaves`: the
        transcription is often an octave off) nearest to `t`, later than `after`."""
        hits = [
            h
            for n in sl.notes
            for o in octaves
            for h in [nearest(by_pitch.get(n.pitch + o), t, within)]
            if h is not None and h > after
        ]
        return min(hits, key=lambda h: abs(h - t)) if hits else None

    # The chroma mapping is right to a few tenths of a second; the notes heard around each
    # slice say how far off it is there (median over its neighbours: a wrong match does not
    # count), then each slice lands on its own attack.
    mapped = np.array([to_song(sl.time) for sl in part.tab])
    lags = np.array([np.nan if (h := heard(sl, t, LAG_SEARCH)) is None else h - t for sl, t in zip(part.tab, mapped)])
    lag = _rolling_median(lags, LAG_SLICES)
    mapped = mapped + lag
    times: list[float] = []
    confirmed = 0
    last = -np.inf
    for sl, t in zip(part.tab, mapped):
        # In the tab's order, each attack used once; the exact pitch first.
        h = heard(sl, t, SNAP_PITCH, after=last + 0.02)
        if h is None:
            h = heard(sl, t, SNAP_ATTACK, after=last + 0.02, octaves=OCTAVES)
        confirmed += h is not None
        if h is None:
            a = nearest(attacks, t, SNAP_ATTACK)
            h = a if a is not None and a > last + 0.02 else float(t)
        h = max(h, last + 0.01)
        times.append(h)
        last = h

    placed = []
    for sl, t in zip(part.tab, times):
        notes = [Note(t, n.pitch, max(to_song(n.time + n.duration) - to_song(n.time), 0.0), n.velocity) for n in sl.notes]
        placed.append(Fingered(t, notes, list(sl.placements), sl.anchor, list(sl.techniques) if sl.techniques else sl.techniques))
    slice_times = np.array([sl.time for sl in part.tab]) if part.tab else np.zeros(1)
    lag_at = lambda t: float(np.interp(t, slice_times, lag)) if part.tab else 0.0  # noqa: E731
    beats = [to_song(b) + lag_at(b) for b in part.beats]
    end = to_song(part.duration) + lag_at(part.duration)
    grid = BeatGrid(beats + [end], part.bars, part.beats_per_bar)
    span = part.tab[-1].time - part.tab[0].time if len(part.tab) > 1 else 0.0
    stretch = (times[-1] - times[0]) / span if span > 0 else 1.0
    return Placement(placed, grid, confirmed / max(len(part.tab), 1), beats[0] if beats else 0.0, end, stretch)


def _rolling_median(values: np.ndarray, half: int) -> np.ndarray:
    """Median of the known values within `half` places of each, NaN-free (the overall median
    where none is known nearby, 0 without any)."""
    known = values[~np.isnan(values)]
    overall = float(np.median(known)) if known.size else 0.0
    out = np.empty_like(values)
    for i in range(len(values)):
        window = values[max(i - half, 0) : i + half + 1]
        window = window[~np.isnan(window)]
        out[i] = np.median(window) if window.size >= 3 else overall
    return out
