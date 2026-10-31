"""Viterbi fingering solver: canonical shapes, known riffs, playability under tempo."""

import random
import time

import pytest

from opensmith_ingest.fingering import FingeringSolver, Note, Tuning, anchors, ascii_tab, playability

STD = Tuning()


def line(pitches, step, start=0.0):
    return [Note(start + k * step, p, step) for k, p in enumerate(pitches)]


def chord(pitches, t=0.0, dur=1.0):
    return [Note(t, p, dur) for p in pitches]


def frets_by_string(sl):
    return {s: f for s, f in sl.placements}


def solve(notes, tuning=STD):
    return FingeringSolver(tuning).solve(notes)


@pytest.mark.parametrize("string,pitch", list(enumerate(STD.strings)))
def test_open_strings_alone_are_played_open(string, pitch):
    (sl,) = solve([Note(0.0, pitch)])
    assert sl.placements == [(string, 0)]


OPEN_CHORDS = {
    "E": ([40, 47, 52, 56, 59, 64], {0: 0, 1: 2, 2: 2, 3: 1, 4: 0, 5: 0}),
    "Em": ([40, 47, 52, 55, 59, 64], {0: 0, 1: 2, 2: 2, 3: 0, 4: 0, 5: 0}),
    "A": ([45, 52, 57, 61, 64], {1: 0, 2: 2, 3: 2, 4: 2, 5: 0}),
    "Am": ([45, 52, 57, 60, 64], {1: 0, 2: 2, 3: 2, 4: 1, 5: 0}),
    "D": ([50, 57, 62, 66], {2: 0, 3: 2, 4: 3, 5: 2}),
    "Dm": ([50, 57, 62, 65], {2: 0, 3: 2, 4: 3, 5: 1}),
    "G": ([43, 47, 50, 55, 59, 67], {0: 3, 1: 2, 2: 0, 3: 0, 4: 0, 5: 3}),
    "C": ([48, 52, 55, 60, 64], {1: 3, 2: 2, 3: 0, 4: 1, 5: 0}),
    "F barre": ([41, 48, 53, 57, 60, 65], {0: 1, 1: 3, 2: 3, 3: 2, 4: 1, 5: 1}),
    "Bm barre": ([47, 54, 59, 62, 66], {1: 2, 2: 4, 3: 4, 4: 3, 5: 2}),
    "E5": ([40, 47, 52], {0: 0, 1: 2, 2: 2}),
}


@pytest.mark.parametrize("name", OPEN_CHORDS)
def test_chords_get_their_standard_shape(name):
    pitches, shape = OPEN_CHORDS[name]
    (sl,) = solve(chord(pitches))
    assert frets_by_string(sl) == shape, f"{name}:\n{ascii_tab([sl])}"


def test_sweet_child_intro_stays_in_twelfth_position():
    # D4 D5 A4 G4 G5 A4 F#5 A4 as eighths at 125 BPM, twice.
    riff = [62, 74, 69, 67, 79, 69, 78, 69] * 2
    tab = solve(line(riff, 0.24))
    expected = [(2, 12), (4, 15), (3, 14), (3, 12), (5, 15), (3, 14), (5, 14), (3, 14)] * 2
    assert [sl.placements[0] for sl in tab] == expected, ascii_tab(tab)
    assert {a.fret for a in anchors(tab)} == {12}


def test_a_minor_pentatonic_runs_stay_in_the_fifth_position_box():
    up = [45, 48, 50, 52, 55, 57, 60, 62, 64, 67, 69, 72]
    tab = solve(line(up + up[::-1][1:], 0.15))
    frets = [sl.placements[0][1] for sl in tab]
    assert all(5 <= f <= 8 for f in frets), ascii_tab(tab)
    assert playability(tab).shifts == 0


def test_smoke_on_the_water_double_stops_on_d_and_g():
    pairs = [(50, 55), (53, 58), (55, 60), (50, 55), (53, 58), (56, 61), (55, 60), (50, 55), (53, 58), (55, 60), (53, 58), (50, 55)]
    times = [0, 0.5, 1.0, 1.75, 2.0, 2.25, 2.5, 3.5, 4.0, 4.5, 5.25, 5.75]
    notes = [n for (a, b), t in zip(pairs, times) for n in chord([a, b], t, 0.4)]
    tab = solve(notes)
    for sl in tab:
        strings = sorted(s for s, _ in sl.placements)
        frets = {f for _, f in sl.placements}
        assert strings == [2, 3] and len(frets) == 1, ascii_tab(tab)
    assert [sl.placements[0][1] for sl in tab] == [0, 3, 5, 0, 3, 6, 5, 0, 3, 5, 3, 0]


def test_fast_octave_line_never_leaves_its_position():
    tab = solve(line([57, 69] * 16, 0.125))
    assert len(playability(tab).positions) == 1, ascii_tab(tab)


def test_sixteenth_note_lines_have_no_big_jumps():
    rng = random.Random(7)
    for trial in range(30):
        pitch, pitches = rng.randint(45, 70), []
        for _ in range(64):
            pitch = min(max(pitch + rng.choice([-5, -4, -3, -2, -1, 1, 2, 3, 4, 5]), 40), 84)
            pitches.append(pitch)
        tab = solve(line(pitches, 0.125))
        p = playability(tab, fast=0.2)
        assert p.max_anchor_jump_fast <= 5, f"trial {trial}: hand jumped {p.max_anchor_jump_fast} frets\n{ascii_tab(tab)}"
        assert p.max_fret_jump < 12
        assert p.max_stretch <= 5


def test_slow_notes_may_change_position():
    # E2 then G5, two seconds apart: no reason to stay anywhere.
    tab = solve([Note(0.0, 40), Note(2.0, 79)])
    assert tab[0].placements == [(0, 0)] and tab[1].placements[0][1] >= 15


def test_drop_d_power_chord_is_three_open_strings():
    drop_d = Tuning((38, 45, 50, 55, 59, 64), "Drop D")
    (sl,) = solve(chord([38, 45, 50]), drop_d)
    assert frets_by_string(sl) == {0: 0, 1: 0, 2: 0}


def test_capo_shifts_the_open_position():
    capo2 = Tuning(capo=2)
    (sl,) = solve([Note(0.0, 42)], capo2)
    assert sl.placements == [(0, 2)]
    with pytest.raises(ValueError):
        solve([Note(0.0, 40)], capo2)


def test_out_of_range_pitch_is_rejected_or_dropped():
    with pytest.raises(ValueError):
        solve([Note(0.0, 30)])
    solver = FingeringSolver(STD)
    tab = solver.solve([Note(0.0, 30), Note(0.5, 45)], strict=False)
    assert [sl.placements for sl in tab] == [[(1, 0)]]
    assert [n.pitch for n in solver.dropped] == [30]


def test_unplayable_chord_keeps_its_loudest_playable_notes():
    solver = FingeringSolver(STD)
    (sl,) = solver.solve([Note(0.0, 40, velocity=0.5), Note(0.0, 42, velocity=0.9), Note(0.0, 47, velocity=0.7)], strict=False)
    assert sorted(n.pitch for n in sl.notes) == [42, 47]
    assert [n.pitch for n in solver.dropped] == [40]


def test_large_transcriptions_solve_fast():
    rng = random.Random(1)
    notes, t, pitch = [], 0.0, 55
    for k in range(3000):
        pitch = min(max(pitch + rng.randint(-4, 4), 41), 80)
        if k % 7 == 0:
            notes += chord([max(pitch - 7, 40), pitch, pitch + 5], t, 0.2)
        else:
            notes.append(Note(t, pitch, 0.1))
        t += rng.choice([0.0625, 0.125, 0.25])
    t0 = time.perf_counter()
    solver = FingeringSolver(STD)
    tab = solver.solve(notes, strict=False)
    elapsed = time.perf_counter() - t0
    assert len(tab) == 3000
    # E2 + F#2 (+ B2) only exist on the low E string: the quieter conflicting note goes.
    assert all(n.pitch in (40, 41, 42, 43, 44) for n in solver.dropped)
    assert elapsed < 3.0, f"{elapsed:.2f} s"


def test_ascii_tab_renders_every_string():
    tab = solve(chord([40, 47, 52, 56, 59, 64]))
    text = ascii_tab(tab)
    assert text.splitlines()[0].startswith("e|") and text.splitlines()[-1].startswith("E|")
    assert "1" in text.splitlines()[2]


def test_chord_shapes_get_the_fingers_guitarists_use():
    from opensmith_ingest.fingering import chord_fingers

    # Fret per string from the low E (-1 not played) → finger (0 open, 1 index … 4 little finger).
    expected = {
        (0, 2, 2, 1, 0, 0): (0, 2, 3, 1, 0, 0),  # E
        (0, 2, 2, 0, 0, 0): (0, 2, 3, 0, 0, 0),  # Em
        (-1, 0, 2, 2, 2, 0): (-1, 0, 1, 2, 3, 0),  # A: no barre, the high E rings
        (-1, 0, 2, 2, 1, 0): (-1, 0, 2, 3, 1, 0),  # Am
        (-1, 3, 2, 0, 1, 0): (-1, 3, 2, 0, 1, 0),  # C
        (-1, -1, 0, 2, 3, 2): (-1, -1, 0, 1, 3, 2),  # D
        (-1, -1, 0, 2, 3, 1): (-1, -1, 0, 2, 3, 1),  # Dm
        (1, 3, 3, 2, 1, 1): (1, 3, 4, 2, 1, 1),  # F: index barre
        (-1, 2, 4, 4, 3, 2): (-1, 1, 3, 4, 2, 1),  # Bm
        (-1, 2, 1, 2, 0, 2): (-1, 2, 1, 3, 0, 4),  # B7
        (3, 5, 5, -1, -1, -1): (1, 3, 4, -1, -1, -1),  # G5
    }
    for frets, fingers in expected.items():
        assert chord_fingers(frets, 1 if max(frets) <= 4 else 0) == fingers, frets
    # Wider than a hand can hold: still a finger for every note.
    assert all(f >= 1 for f in chord_fingers((1, -1, -1, -1, -1, 9), 0) if f != -1)
