"""Guitar stem → notes with Spotify's Basic Pitch (CoreML on macOS), then clean-up for a chart.

Basic Pitch is polyphonic and generous: it fragments long notes, reports strong harmonics
(an octave or a twelfth above, and distortion products of power chords) as extra notes, and
keeps faint bleed from other instruments. The clean-up merges fragments, keeps only notes that
start on an actual attack of the guitar (or are strong enough to be legato notes), removes
harmonic ghosts and quiet notes, and keeps the pitches the instrument can play.

With the guitar stem itself (`stem.GuitarStem`) it also drops notes where the guitar is silent,
finds attacks section by section, and checks every group of simultaneous notes against the
period of the waveform (YIN): a low note with a weak fundamental, which Basic Pitch writes an
octave up (often along with the right one), goes back down.
"""

from __future__ import annotations

import logging
import warnings
from dataclasses import dataclass
from pathlib import Path

import numpy as np

from .fingering import Note, Tuning
from .stem import GuitarStem


@dataclass(frozen=True)
class RawNote:
    start: float
    end: float
    pitch: int
    amplitude: float


def basic_pitch(path: Path, onset_threshold: float = 0.5, frame_threshold: float = 0.3, min_note_ms: float = 60.0) -> list[RawNote]:
    logging.getLogger().setLevel(logging.ERROR)
    warnings.filterwarnings("ignore")
    from basic_pitch import ICASSP_2022_MODEL_PATH
    from basic_pitch.inference import predict

    _, _, events = predict(
        str(path),
        ICASSP_2022_MODEL_PATH,
        onset_threshold=onset_threshold,
        frame_threshold=frame_threshold,
        minimum_note_length=min_note_ms,
        minimum_frequency=60.0,
        maximum_frequency=1500.0,
        multiple_pitch_bends=False,
        melodia_trick=True,
    )
    return sorted((RawNote(float(s), float(e), int(p), float(a)) for s, e, p, a, *_ in events), key=lambda n: (n.start, n.pitch))


def clean(
    raw: list[RawNote],
    tuning: Tuning = Tuning(),
    onsets: np.ndarray | None = None,
    stem: GuitarStem | None = None,
    min_amplitude: float = 0.15,
    merge_gap: float = 0.03,
    attack_window: float = 0.06,
    legato_amplitude: float = 0.55,
) -> list[Note]:
    """Merges fragments, drops notes without an attack, harmonic ghosts, quiet and unplayable notes.

    `onsets` (from `attacks`, or the stem's) enables attack gating: a note must start within
    `attack_window` of an attack unless it is loud enough (`legato_amplitude`) to be a hammer-on
    or pull-off. With the `stem`, notes where it is silent go and simultaneous notes are checked
    against its pitch (`consolidate`).
    """
    notes = [n for n in raw if tuning.playable(n.pitch) and n.amplitude >= min_amplitude]
    if stem is not None:
        notes = [n for n in notes if stem.audible(n.start)]
        if onsets is None:
            onsets = stem.attacks

    merged: list[RawNote] = []
    last_by_pitch: dict[int, int] = {}
    for n in notes:
        k = last_by_pitch.get(n.pitch)
        if k is not None and n.start - merged[k].end <= merge_gap:
            m = merged[k]
            merged[k] = RawNote(m.start, max(m.end, n.end), m.pitch, max(m.amplitude, n.amplitude))
            continue
        last_by_pitch[n.pitch] = len(merged)
        merged.append(n)

    def ghost(n: RawNote) -> bool:
        for m in merged:
            if m is n or abs(m.start - n.start) > 0.05:
                continue
            if n.pitch - m.pitch in (12, 19, 24) and m.amplitude >= 1.3 * n.amplitude:
                return True
        return False

    def attacked(n: RawNote) -> bool:
        if onsets is None or len(onsets) == 0 or n.amplitude >= legato_amplitude:
            return True
        return bool(np.min(np.abs(onsets - n.start)) <= attack_window)

    kept = [n for n in merged if attacked(n) and not ghost(n)]
    out = [Note(n.start, n.pitch, n.end - n.start, n.amplitude) for n in sorted(kept, key=lambda n: (n.start, n.pitch))]
    return consolidate(out, stem, tuning) if stem is not None else out


# Notes starting this close together are struck together.
SLICE = 0.04
# YIN frames after a slice's start (s): past the pick noise, within the note; and just before
# it, to know what was already ringing.
YIN_AT = (0.03, 0.06, 0.09, 0.12)
YIN_BEFORE = (-0.07, -0.04)
# A YIN frame is trusted below this aperiodicity.
YIN_CLEAR = 0.2
# A slice whose neighbours (within MONO_SPAN s, at least MONO_MIN of them) are single notes
# for MONO_SHARE of them is part of a single-note line.
MONO_SPAN = 2.0
MONO_MIN = 4
MONO_SHARE = 0.8
# The same pitch again this soon is one note split in two (s): faster than sixteenths at 150 BPM.
REPEAT = 0.1
# Low string of standard tuning (E2).
STANDARD_LOW = 40


def consolidate(notes: list[Note], stem: GuitarStem, tuning: Tuning) -> list[Note]:
    """Simultaneous notes checked against the stem's pitch (YIN):

    - a single note goes one octave down when YIN hears it there (a weak fundamental put it an
      octave up);
    - in a single-note line, a note doubled at the octave becomes one note: the one YIN hears,
      the louder otherwise;
    - chords, octave shapes among them, are left to the chord clean-up.

    YIN hears the strongest periodicity, which may be a note that was already ringing (an open
    string under an arpeggio, the bass bleeding into the stem): its reading counts only when,
    just before the attack, it clearly heard another pitch; and no note goes below the lowest
    the part plays (E2 in any case).
    """
    from .tunings import lowest_played

    slices: list[list[Note]] = []
    for n in sorted(notes, key=lambda n: (n.time, n.pitch)):
        if slices and n.time - slices[-1][0].time <= SLICE:
            slices[-1].append(n)
        else:
            slices.append([n])
    if not slices:
        return []
    # Down to E2 always (standard tuning); lower only where the part already goes.
    floor = min(lowest_played(notes) or STANDARD_LOW, STANDARD_LOW)
    offsets = np.array(YIN_AT)
    at = YIN_AT + YIN_BEFORE
    midi, aperiodicity = stem.pitch([sl[0].time + d for sl in slices for d in at])
    midi = midi.reshape(len(slices), len(at))
    aperiodicity = aperiodicity.reshape(len(slices), len(at))
    after = slice(0, len(YIN_AT))
    before = slice(len(YIN_AT), len(at))
    starts = np.array([sl[0].time for sl in slices])
    single = np.array([len({n.pitch % 12 for n in sl}) == 1 for sl in slices])

    out: list[Note] = []
    for i, sl in enumerate(slices):
        length = max(n.duration for n in sl)
        clear = (aperiodicity[i][after] < YIN_CLEAR) & (offsets <= max(length, 0.06) + 0.02)
        heard = _heard(midi[i][after][clear], aperiodicity[i][after][clear])
        ringing = _heard(midi[i][before], aperiodicity[i][before], need=1.0)
        # A pitch already heard just before the attack is a note still ringing, not this one.
        if heard is not None and ringing is not None and round(ringing) == round(heard):
            heard = None
        classes = {n.pitch % 12 for n in sl}
        if len(sl) == 1:
            out.append(_merged(sl, _octave(sl[0].pitch, heard, tuning, floor)))
            continue
        near = np.abs(starts - starts[i]) <= MONO_SPAN
        near[i] = False
        in_line = near.sum() >= MONO_MIN and single[near].mean() >= MONO_SHARE
        if len(classes) == 1 and in_line:
            lowest = min(n.pitch for n in sl)
            moved = _octave(lowest, heard, tuning, floor)
            pitch = moved if heard is not None and round(heard) % 12 == lowest % 12 else max(sl, key=lambda n: n.velocity).pitch
            out.append(_merged(sl, pitch))
            continue
        out.extend(sl)
    return _merge_repeats(out)


def _heard(values: np.ndarray, aperiodicity: np.ndarray, need: float = 0.75) -> float | None:
    """The pitch YIN hears over clear frames (aperiodicity under YIN_CLEAR), if `need` of them
    agree on its class: the lowest octave read, since a weak fundamental hides it in some frames
    (the reverse, a period read twice too long, takes a much worse fit than YIN accepts)."""
    ok = aperiodicity < YIN_CLEAR
    values = values[ok]
    if values.size < 2:
        return None
    classes = np.round(values).astype(int) % 12
    common = np.bincount(classes, minlength=12).argmax()
    same = values[classes == common]
    if same.size < need * values.size:
        return None
    return float(same.min())


def _octave(pitch: int, heard: float | None, tuning: Tuning, floor: int) -> int:
    """`pitch` one octave down when YIN hears it there (a weak fundamental put it an octave up),
    never below `floor`. Never up: YIN reading the octave above is what a weak fundamental does,
    not evidence."""
    if heard is None:
        return pitch
    r = round(heard)
    if r == pitch - 12 and abs(heard - r) < 0.4 and r >= floor and tuning.playable(r):
        return r
    return pitch


def _merged(group: list[Note], pitch: int) -> Note:
    start = min(n.time for n in group)
    end = max(n.time + n.duration for n in group)
    return Note(start, pitch, end - start, max(n.velocity for n in group))


def _merge_repeats(notes: list[Note]) -> list[Note]:
    """Same pitch again within REPEAT: the second is a fragment of the first."""
    out: list[Note] = []
    last: dict[int, int] = {}
    for n in sorted(notes, key=lambda n: (n.time, n.pitch)):
        k = last.get(n.pitch)
        if k is not None and n.time - out[k].time < REPEAT:
            o = out[k]
            out[k] = Note(o.time, o.pitch, max(o.time + o.duration, n.time + n.duration) - o.time, max(o.velocity, n.velocity))
            continue
        last[n.pitch] = len(out)
        out.append(n)
    return out


def recheck(notes: list[Note], stem: GuitarStem, tuning: Tuning) -> list[Note]:
    """Notes after the repeat vote, which copies notes from one repeat into the others: those
    landing where the guitar is silent go, the rest is consolidated against this repeat's audio."""
    return consolidate([n for n in notes if stem.audible(n.time)], stem, tuning)
