"""Chord naming from the notes of a shape (bass decides inversions: C/E, G/B, D/F#)."""

from __future__ import annotations

from itertools import combinations

NOTE_NAMES = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B"]

# Interval sets above the root, most common reading first.
QUALITIES: list[tuple[frozenset[int], str]] = [
    (frozenset({0, 4, 7}), ""),
    (frozenset({0, 3, 7}), "m"),
    (frozenset({0, 7}), "5"),
    (frozenset({0, 4, 7, 10}), "7"),
    (frozenset({0, 3, 7, 10}), "m7"),
    (frozenset({0, 4, 7, 11}), "maj7"),
    (frozenset({0, 5, 7}), "sus4"),
    (frozenset({0, 2, 7}), "sus2"),
    (frozenset({0, 2, 4, 7}), "add9"),
    (frozenset({0, 4, 7, 9}), "6"),
    (frozenset({0, 3, 7, 9}), "m6"),
    (frozenset({0, 5, 7, 10}), "7sus4"),
    (frozenset({0, 3, 6}), "dim"),
    (frozenset({0, 3, 6, 9}), "dim7"),
    (frozenset({0, 3, 6, 10}), "m7b5"),
    (frozenset({0, 4, 8}), "aug"),
    (frozenset({0, 4, 10}), "7"),
    (frozenset({0, 3, 10}), "m7"),
    (frozenset({0, 4, 11}), "maj7"),
    (frozenset({0, 4}), ""),
    (frozenset({0, 3}), "m"),
    (frozenset({0}), ""),
]


def chord_name(pitches: list[int] | tuple[int, ...]) -> str:
    """Name of the chord formed by MIDI `pitches` ("E", "Am7", "C/E", "E5")."""
    if not pitches:
        return ""
    bass = min(pitches) % 12
    classes = {p % 12 for p in pitches}
    for root in [bass, *sorted(classes - {bass})]:
        intervals = frozenset((pc - root) % 12 for pc in classes)
        for quality, suffix in QUALITIES:
            if intervals == quality:
                name = NOTE_NAMES[root] + suffix
                return name if root == bass else f"{name}/{NOTE_NAMES[bass]}"
    return NOTE_NAMES[bass] + "*"


# A note this far above the bass that repeats a pitch class of the chord is a harmonic.
HARMONIC_ABOVE = 31


def chord_core(pitches: list[int], loudness: list[float]) -> list[int]:
    """Indices of the notes a guitarist actually plays in a transcribed chord: harmonics stacked
    far above the bass go, then the fewest, quietest notes until the rest reads as a chord
    (C5, Am7, E/G#…). The bass always stays."""
    idx = list(range(len(pitches)))
    if len(idx) < 2:
        return idx
    bass = min(pitches)
    keep = [
        i for i in idx if not (pitches[i] - bass > HARMONIC_ABOVE and any(pitches[j] % 12 == pitches[i] % 12 and pitches[j] < pitches[i] for j in idx))
    ]
    if not chord_name([pitches[i] for i in keep]).endswith("*"):
        return keep
    lowest = min(keep, key=lambda i: pitches[i])
    others = [i for i in keep if i != lowest]
    for k in range(1, len(others)):
        for drop in sorted(combinations(others, k), key=lambda d: sum(loudness[i] for i in d)):
            rest = [i for i in keep if i not in drop]
            if not chord_name([pitches[i] for i in rest]).endswith("*"):
                return rest
    return [lowest]
