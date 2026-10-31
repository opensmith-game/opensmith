"""Repeated phrases played the same way: a vote across the repeats of each phrase.

Transcribing a distorted guitar, each repeat of a riff comes out a little different: a note
missed here, a harmonic ghost there. The errors differ from one repeat to the next, the riff
does not. So a note that at least half of the repeats agree on (same pitch, same place in the
bar, give or take a twelfth of a beat) is the riff, the rest is noise; and every repeat is then
written with the same notes and the same fingering, as a human charter would. A single note
heard in different octaves across the repeats counts as one, in the lowest octave a third of
them agree on (missed weak fundamentals put notes an octave up, hardly ever down).
"""

from __future__ import annotations

import math
from collections import Counter, defaultdict
from collections.abc import Callable
from dataclasses import dataclass

import numpy as np

from .fingering import Fingered, Note

# Positions per beat: sixteenths (3) and triplets (4) both fall on this grid.
UNITS = 12
# A phrase heard twice cannot outvote: a note only one repeat has is kept if it is at least
# this loud, relative to the phrase's median note.
PAIR_LOUDNESS = 0.75


@dataclass
class Vote:
    """What the vote did, summed over every repeat of every repeated phrase."""

    groups: int = 0
    kept: int = 0
    dropped: int = 0
    added: int = 0


def _support(found: list[dict[tuple[int, int], tuple[float, float]]], key: tuple[int, int]) -> list[tuple[float, float]]:
    """(duration, velocity) of each repeat holding `key`'s pitch within one position of it."""
    pos, pitch = key
    out = []
    for repeat in found:
        hit = next((repeat[(p, pitch)] for p in (pos, pos - 1, pos + 1) if (p, pitch) in repeat), None)
        if hit is not None:
            out.append(hit)
    return out


def _agree_octaves(notes: list[Note], groups: dict[str, list[tuple[float, float]]], position: Callable[[float], float]) -> list[Note]:
    """A single note written in different octaves across the repeats of a phrase (an octave up
    where the transcription missed a weak fundamental) is set, in every repeat, to the lowest
    octave at least a third of them have, so that the vote counts it once."""
    notes = list(notes)
    for spans in groups.values():
        if len(spans) < 2:
            continue
        by_key: dict[tuple[int, int], list[int]] = defaultdict(list)
        for a, b in spans:
            p0 = position(a)
            by_pos: dict[int, list[int]] = defaultdict(list)
            for i, n in enumerate(notes):
                if a <= n.time < b:
                    by_pos[round((position(n.time) - p0) * UNITS)].append(i)
            for pos, idx in by_pos.items():
                if len(idx) == 1:
                    by_key[(pos, notes[idx[0]].pitch % 12)].append(idx[0])
        for found in by_key.values():
            octaves = Counter(notes[i].pitch for i in found)
            agreed = [p for p, k in octaves.items() if 3 * k >= len(found)]
            if len(octaves) < 2 or not agreed:
                continue  # one octave already, or no third of the repeats agree: left as heard
            target = min(agreed)
            for i in found:
                n = notes[i]
                notes[i] = Note(n.time, target, n.duration, n.velocity)
    return notes


def consensus(
    notes: list[Note],
    phrases: list[tuple[float, float, str]],
    position: Callable[[float], float],
    time_at: Callable[[float], float],
) -> tuple[list[Note], Vote]:
    """`phrases`: (start, end, name), repeats sharing a name. `position(t)` is the (fractional)
    beat index of time `t`, `time_at` its inverse: repeats are compared beat for beat, tempo
    drift included."""
    groups: dict[str, list[tuple[float, float]]] = defaultdict(list)
    for a, b, name in phrases:
        if name != "rest":
            groups[name].append((a, b))
    notes = _agree_octaves(notes, groups, position)
    voted: set[int] = set()
    out: list[Note] = []
    vote = Vote()
    for spans in groups.values():
        if len(spans) < 2:
            continue
        vote.groups += 1
        found: list[dict[tuple[int, int], tuple[float, float]]] = []
        for a, b in spans:
            p0 = position(a)
            repeat: dict[tuple[int, int], tuple[float, float]] = {}
            for i, n in enumerate(notes):
                if a <= n.time < b:
                    voted.add(i)
                    key = (round((position(n.time) - p0) * UNITS), n.pitch)
                    length = position(n.time + n.duration) - position(n.time)
                    if key not in repeat or repeat[key][0] < length:
                        repeat[key] = (length, n.velocity)
            found.append(repeat)
        n = len(spans)
        velocities = [v for repeat in found for _, v in repeat.values()]
        loud = float(np.median(velocities)) if velocities else 0.0
        candidates = sorted({key for repeat in found for key in repeat})
        support = {key: _support(found, key) for key in candidates}
        kept = []
        for key in candidates:
            backers = support[key]
            pos, pitch = key
            # One place per note: a neighbour with more support (or as much, earlier) wins.
            neighbours = [support.get((pos + d, pitch), []) for d in (-1, 1)]
            if any(len(s) > len(backers) or (len(s) == len(backers) and d < 0) for d, s in zip((-1, 1), neighbours) if s):
                continue
            count = len(backers)
            if n >= 3:
                ok = count >= math.ceil(n / 2)
            else:
                ok = count == 2 or float(np.median([v for _, v in backers])) >= PAIR_LOUDNESS * loud
            if ok:
                kept.append((key, float(np.median([d for d, _ in backers])), float(np.median([v for _, v in backers]))))
        for (a, b), repeat in zip(spans, found):
            p0, length = position(a), position(b) - position(a)
            here = {(p, q) for (p, q) in repeat}
            matched = 0
            for (pos, pitch), dur, vel in kept:
                rel = pos / UNITS
                if rel >= length - 1e-6:
                    continue
                t = time_at(p0 + rel)
                out.append(Note(t, pitch, max(time_at(p0 + rel + dur) - t, 0.0), vel))
                if any((pos + d, pitch) in here for d in (-1, 0, 1)):
                    matched += 1
                else:
                    vote.added += 1
                vote.kept += 1
            vote.dropped += max(len(repeat) - matched, 0)
    out += [note for i, note in enumerate(notes) if i not in voted]
    out.sort(key=lambda note: (note.time, note.pitch))
    return out, vote


def unify_fingering(
    tab: list[Fingered], phrases: list[tuple[float, float, str]], position: Callable[[float], float]
) -> int:
    """Repeats holding the same notes take the fingering most of them got. Returns how many
    slices changed."""
    groups: dict[str, list[list[int]]] = defaultdict(list)
    for a, b, name in phrases:
        if name != "rest":
            groups[name].append([k for k, sl in enumerate(tab) if a <= sl.time < b])
    changed = 0
    for repeats in groups.values():
        if len(repeats) < 2:
            continue

        def shape(slices: list[int]) -> tuple:
            p0 = position(tab[slices[0]].time) if slices else 0.0
            return tuple((round((position(tab[k].time) - p0) * UNITS), tuple(sorted(n.pitch for n in tab[k].notes))) for k in slices)

        by_shape: dict[tuple, list[list[int]]] = defaultdict(list)
        for slices in repeats:
            if slices:
                by_shape[shape(slices)].append(slices)
        for same in by_shape.values():
            if len(same) < 2:
                continue
            fingerings = Counter(tuple(tuple(tab[k].placements) for k in slices) for slices in same)
            best = fingerings.most_common(1)[0][0]
            source = next(slices for slices in same if tuple(tuple(tab[k].placements) for k in slices) == best)
            for slices in same:
                for src, dst in zip(source, slices):
                    if tab[dst].placements == tab[src].placements:
                        continue
                    by_pitch: dict[int, list[tuple[int, int]]] = defaultdict(list)
                    for note, place in zip(tab[src].notes, tab[src].placements):
                        by_pitch[note.pitch].append(place)
                    tab[dst].placements = [by_pitch[note.pitch].pop(0) for note in tab[dst].notes]
                    tab[dst].anchor = tab[src].anchor
                    changed += 1
    return changed
