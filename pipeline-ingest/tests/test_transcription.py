"""Basic Pitch's notes checked against the guitar stem: octaves, doubles, silence, fragments."""

import numpy as np

from opensmith_ingest.fingering import Tuning
from opensmith_ingest.stem import RATE, GuitarStem
from opensmith_ingest.transcription import RawNote, clean

from test_stem import PLAIN, WEAK_FUNDAMENTAL, add, tone

STANDARD = Tuning()


def line(pitches: list[int], step: float, partials: dict[int, float], t60: float = 0.25, start: float = 0.2):
    """A single-note line as heard (stem) and its times."""
    times = [start + step * k for k in range(len(pitches))]
    y = np.zeros(int((times[-1] + 1.0) * RATE))
    for t, p in zip(times, pitches):
        add(y, tone(p, step * 0.95, partials, t60), t)
    return GuitarStem(y), np.array(times)


def raw(times, pitches, dur=0.2, amp=0.5):
    return [RawNote(float(t), float(t) + dur, p, amp) for t, p in zip(times, pitches)]


RIFF = [40, 43, 45, 40, 47, 45, 43, 40] * 2  # E2 G2 A2 E2 B2 A2 G2 E2


def test_a_low_note_written_an_octave_up_goes_back_down():
    stem, times = line(RIFF, 0.3, WEAK_FUNDAMENTAL)
    # Basic Pitch hears the strong octave of every note.
    notes = clean(raw(times, [p + 12 for p in RIFF]), STANDARD, onsets=times, stem=stem)
    assert [n.pitch for n in notes] == RIFF


def test_a_note_doubled_at_the_octave_in_a_line_becomes_one():
    stem, times = line(RIFF, 0.3, WEAK_FUNDAMENTAL)
    doubled = raw(times, RIFF) + raw(times[::3], [p + 12 for p in RIFF[::3]], amp=0.6)
    notes = clean(doubled, STANDARD, onsets=times, stem=stem)
    assert [n.pitch for n in notes] == RIFF


def test_a_ringing_string_does_not_pull_the_next_note_down():
    # Open E2 let ring under E3 and B3 (an arpeggio): YIN hears the low E all along.
    y = np.zeros(3 * RATE)
    add(y, tone(40, 2.8, PLAIN, t60=6.0), 0.1)
    add(y, 0.6 * tone(52, 1.2, PLAIN), 0.8)
    add(y, 0.6 * tone(59, 1.0, PLAIN), 1.6)
    stem = GuitarStem(y)
    times = np.array([0.1, 0.8, 1.6])
    notes = clean(raw(times, [40, 52, 59], dur=0.6), STANDARD, onsets=times, stem=stem)
    assert [n.pitch for n in notes] == [40, 52, 59]


def test_chords_and_octave_shapes_are_left_alone():
    y = np.zeros(4 * RATE)
    chords = [[40, 47, 52], [45, 57], [43, 50, 55], [45, 57], [40, 47, 52]]
    times = np.array([0.2 + 0.6 * k for k in range(len(chords))])
    for t, c in zip(times, chords):
        for p in c:
            add(y, 0.4 * tone(p, 0.55, PLAIN), t)
    stem = GuitarStem(y)
    played = [RawNote(float(t), float(t) + 0.5, p, 0.5) for t, c in zip(times, chords) for p in c]
    notes = clean(played, STANDARD, onsets=times, stem=stem)
    assert sorted((round(n.time, 2), n.pitch) for n in notes) == sorted((round(n.start, 2), n.pitch) for n in played)


def test_notes_where_the_guitar_is_silent_go():
    stem, times = line([45, 47, 48], 0.4, PLAIN, start=2.0)
    hallucinated = raw([0.5, 1.0], [44, 46], amp=0.6)  # Basic Pitch on near-silence
    notes = clean(hallucinated + raw(times, [45, 47, 48]), STANDARD, onsets=np.r_[0.5, 1.0, times], stem=stem)
    assert [n.pitch for n in notes] == [45, 47, 48]


def test_a_note_split_in_two_is_one_note():
    stem, times = line([45, 50], 0.5, PLAIN)
    split = raw(times, [45, 50], dur=0.04) + raw(times[:1] + 0.07, [45], dur=0.3)
    notes = clean(split, STANDARD, onsets=times, stem=stem)
    assert [(round(n.time, 2), n.pitch) for n in notes] == [(round(times[0], 2), 45), (round(times[1], 2), 50)]
    assert notes[0].duration > 0.3
