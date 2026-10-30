"""Charts as a human would write them: repeats voted, fingering unified, a steady hand, clean chords."""

from opensmith_ingest.builder import BeatGrid
from opensmith_ingest.consensus import consensus, unify_fingering
from opensmith_ingest.fingering import Fingered, Note, Tuning, anchors
from opensmith_ingest.harmony import chord_core

GRID = BeatGrid.regular(120, 30.0)  # 0.5 s beats
PHRASES = [(0.0, 2.0, "A"), (2.0, 4.0, "A"), (4.0, 6.0, "A"), (6.0, 8.0, "B")]
PITCHES = [40, 43, 45, 43, 40, 43, 45, 47]


def riff(start: float, grid: BeatGrid = GRID) -> list[Note]:
    """A bar of eighth notes from `start`, following the grid's tempo."""
    p0 = grid.position(start)
    times = [grid.time_at(p0 + k / 2) for k in range(9)]
    return [Note(t, p, 0.9 * (u - t), 0.8) for t, u, p in zip(times, times[1:], PITCHES)]


def relative(notes, a, b):
    return [(round(n.time - a, 3), n.pitch) for n in notes if a <= n.time < b]


def test_repeats_agree_on_the_riff_whatever_each_transcription_got():
    first, second, third = riff(0.0), riff(2.0), riff(4.0)
    notes = first + second[:3] + second[4:] + third + [Note(4.25, 76, 0.2, 0.3)]  # one miss, one ghost
    out, vote = consensus(notes, PHRASES, GRID.position, GRID.time_at)
    expected = relative(first, 0.0, 2.0)
    for a, b, _ in PHRASES[:3]:
        assert relative(out, a, b) == expected
    assert (vote.groups, vote.dropped, vote.added) == (1, 1, 1)


def test_the_vote_follows_tempo_changes():
    beats = [0.0]
    for k in range(40):
        beats.append(beats[-1] + 0.5 + 0.01 * k)  # slowing down
    grid = BeatGrid(beats, list(range(0, 41, 4)))
    a, b, c = (grid.beats[i] for i in (0, 4, 8))
    phrases = [(a, b, "A"), (b, c, "A"), (c, grid.beats[12], "A")]
    second = riff(b, grid)
    notes = riff(a, grid) + second[:5] + second[6:] + riff(c, grid)
    out, _ = consensus(notes, phrases, grid.position, grid.time_at)
    restored = [n for n in out if b <= n.time < c]
    assert [n.pitch for n in restored] == PITCHES
    assert abs(restored[5].time - second[5].time) < 1e-6, "back where that repeat's beat is"


def test_a_note_heard_once_in_two_repeats_stays_only_if_loud():
    first, second = riff(0.0), riff(2.0)
    quiet, loud = Note(1.75, 50, 0.2, 0.2), Note(3.75, 52, 0.2, 0.8)
    out, _ = consensus(first + second + [quiet, loud], PHRASES[:2], GRID.position, GRID.time_at)
    assert (1.75, 50) not in {(n.time, n.pitch) for n in out}
    assert {(1.75, 52), (3.75, 52)} <= {(round(n.time, 3), n.pitch) for n in out}


def test_repeats_take_the_fingering_most_of_them_got():
    def repeat(start, a_string):
        a = (1, 0) if a_string else (0, 5)  # A2: open A string, or 5th fret of the low E
        return [Fingered(start, [Note(start, 40, 0.2)], [(0, 0)], 1), Fingered(start + 0.25, [Note(start + 0.25, 45, 0.2)], [a], 1)]

    tab = repeat(0.0, True) + repeat(2.0, False) + repeat(4.0, True)
    assert unify_fingering(tab, PHRASES[:3], GRID.position) == 1
    assert [sl.placements for sl in tab] == [[(0, 0)], [(1, 0)]] * 3


def test_the_hand_stays_put_through_a_short_excursion():
    tuning = Tuning()

    def sl(t, *places):
        return Fingered(t, [], list(places), min(f for _, f in places if f > 0) if any(f > 0 for _, f in places) else 1)

    tab = [sl(0.0, (0, 5)), sl(0.25, (1, 7)), sl(0.5, (2, 9)), sl(0.7, (1, 5)), sl(1.0, (4, 0)), sl(1.25, (2, 7))]
    positions = anchors(tab, tuning)
    assert [(a.fret, a.width) for a in positions] == [(5, 5)], "a 9th-fret note held 0.2 s: one wider box"

    moved = [sl(0.0, (0, 3)), sl(0.5, (1, 5)), sl(1.0, (2, 6)), sl(2.0, (3, 10)), sl(2.5, (3, 12)), sl(3.0, (4, 13))]
    assert [a.fret for a in anchors(moved, tuning)] == [3, 10]


def test_a_transcribed_chord_keeps_what_a_guitarist_plays():
    # G2 open and a G5 three octaves up: the harmonic goes.
    assert chord_core([43, 79], [0.8, 0.4]) == [0]
    # C5 with a quiet C#: the stray note goes.
    assert chord_core([36, 43, 48, 49], [0.9, 0.8, 0.7, 0.2]) == [0, 1, 2]
    # A real open E major chord stays whole.
    assert chord_core([40, 47, 52, 56, 59, 64], [0.7] * 6) == [0, 1, 2, 3, 4, 5]


def test_beat_positions_and_times_are_inverse():
    for t in (-0.3, 0.0, 0.26, 7.9, 31.0):
        assert abs(GRID.time_at(GRID.position(t)) - t) < 1e-9


def test_a_note_heard_in_two_octaves_across_repeats_counts_once():
    # The third note of the riff (A2) comes out an octave up in one repeat of three.
    first, second, third = riff(0.0), riff(2.0), riff(4.0)
    second[2] = Note(second[2].time, second[2].pitch + 12, second[2].duration, second[2].velocity)
    out, _ = consensus(first + second + third, PHRASES, GRID.position, GRID.time_at)
    for a, b, _ in PHRASES[:3]:
        assert relative(out, a, b) == relative(first, 0.0, 2.0)
    # Up in two repeats of three: still a third at the low octave, which wins.
    second[2], third[2] = (Note(n.time, 57, n.duration, n.velocity) for n in (second[2], third[2]))
    out, _ = consensus(first + second + third, PHRASES, GRID.position, GRID.time_at)
    assert all(relative(out, a, b) == relative(first, 0.0, 2.0) for a, b, _ in PHRASES[:3])
