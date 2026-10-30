"""Common guitar tunings and automatic tuning choice from transcribed notes.

Pitches alone cannot say how a guitarist tuned: a C5 power chord sounds the same in Drop C and
in C standard. What they do say is how low the guitar must go. The detection therefore finds
the lowest note actually played — at least `MIN_NOTES` notes and `MIN_SHARE` of the part, so a
stray note bleeding from the bass does not count — and picks a tuning whose low string reaches
it: Eb standard for Eb; for D, C# and C either the drop tuning (one-finger power chords, the
usual reason to tune down in rock) or every string down (a standard tuning moved down, as
many grunge bands do). The way the part is played tells them apart: power chords on
the low string mean a drop tuning; otherwise the open second string gives it away, a fifth
above the low string in a drop tuning, a fourth above in a standard one, and riffs lean on it.
Whatever the tuning, the chart keeps the recorded pitches, so the player is always in tune with
the song; only the fingering depends on it.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass

from .fingering import Note, Tuning, group_slices

COMMON: dict[str, tuple[int, ...]] = {
    "E Standard": (40, 45, 50, 55, 59, 64),
    "Eb Standard": (39, 44, 49, 54, 58, 63),
    "Drop D": (38, 45, 50, 55, 59, 64),
    "D Standard": (38, 43, 48, 53, 57, 62),
    "Drop C#": (37, 44, 49, 54, 58, 63),
    "Drop C": (36, 43, 48, 53, 57, 62),
    "C Standard": (36, 41, 46, 51, 55, 60),
    "C# Standard": (37, 42, 47, 52, 56, 61),
    "Drop B": (35, 42, 47, 52, 56, 61),
    "B Standard": (35, 40, 45, 50, 54, 59),
}

PITCH_NAMES = ("C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B")
# Six-string tunings that are neither standard nor drop, by each string's offset from E standard.
OPEN = {
    (-2, -2, 0, 0, 0, -2): "Open G",
    (-2, 0, 0, -1, -2, -2): "Open D",
    (0, 2, 2, 1, 0, 0): "Open E",
    (0, 0, 2, 2, 2, 0): "Open A",
    (-4, -2, -2, 0, 1, 0): "Open C",
    (-2, 0, 0, 0, -2, -2): "DADGAD",
    (-2, 0, 0, 0, 0, -2): "Double Drop D",
}


def name_of(strings: tuple[int, ...], standard: tuple[int, ...]) -> str:
    """What a guitarist calls a tuning, from the strings' pitches and the instrument's standard
    tuning: E Standard, Eb Standard, Drop D, Drop C#, Open G…, else its notes from the low string."""
    offsets = tuple(s - t for s, t in zip(strings, standard))
    low = PITCH_NAMES[strings[0] % 12]
    if len(set(offsets)) == 1:
        return f"{low} Standard"
    if len(set(offsets[1:])) == 1 and offsets[0] == offsets[1] - 2:
        return f"Drop {low}"
    if len(strings) == 6 and offsets in OPEN:
        return OPEN[offsets]
    return " ".join(PITCH_NAMES[s % 12] for s in strings)


# Most common tuning for each lowest string, and the standard tuning moved down to it.
BY_LOWEST = {39: "Eb Standard", 38: "Drop D", 37: "Drop C#", 36: "Drop C", 35: "Drop B"}
ALL_DOWN = {38: "D Standard", 37: "C# Standard", 36: "C Standard", 35: "B Standard"}
# Drop tuning when this share of the chords and notes rooted on the low string are power
# chords; standard moved down when the note a fourth above the low string is played at least
# this much relative to the fifth above it.
POWER_SHARE = 0.25
OPEN_RATIO = 0.5

# Range wide enough to keep every note of any common tuning while analysing a transcription.
ANALYSIS = Tuning((35, 40, 45, 50, 55, 59), "analysis", frets=30)

MIN_NOTES = 3
# Bass bleeding into the guitar stem leaves a few low notes per minute: they do not count.
MIN_SHARE = 0.015


@dataclass(frozen=True)
class TuningChoice:
    tuning: Tuning
    lowest_played: int | None
    below_standard: int


def by_name(name: str) -> Tuning:
    return Tuning(COMMON[name], name)


def lowest_played(notes: list[Note]) -> int | None:
    """Lowest pitch with enough notes at or below it to be part of the performance."""
    if not notes:
        return None
    need = max(MIN_NOTES, MIN_SHARE * len(notes))
    seen = 0
    for pitch, count in sorted(Counter(n.pitch for n in notes).items()):
        seen += count
        if seen >= need:
            return pitch
    return None


def all_strings_down(notes: list[Note], low: int) -> bool:
    """Every string tuned down rather than the low one dropped (see the module docstring)."""
    rooted = [sl for sl in group_slices(notes, 0.035) if low <= min(n.pitch for n in sl) <= low + 7]
    power = sum(1 for sl in rooted if any(n.pitch - min(m.pitch for m in sl) == 7 for n in sl))
    if rooted and power >= POWER_SHARE * len(rooted):
        return False
    fourth = sum(1 for n in notes if n.pitch == low + 5)
    fifth = sum(1 for n in notes if n.pitch == low + 7)
    return fourth > 0 and fourth >= OPEN_RATIO * fifth


def detect(notes: list[Note]) -> TuningChoice:
    low = lowest_played(notes)
    below = sum(1 for n in notes if n.pitch < COMMON["E Standard"][0])
    if low is None or low >= COMMON["E Standard"][0]:
        name = "E Standard"
    else:
        name = BY_LOWEST.get(low, "Drop B")
        if low in ALL_DOWN and all_strings_down(notes, low):
            name = ALL_DOWN[low]
    return TuningChoice(by_name(name), low, below)
