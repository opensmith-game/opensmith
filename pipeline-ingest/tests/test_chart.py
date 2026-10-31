"""Chart generation: quantization, difficulty levels, phrases, chord shapes, schema."""

import json
import random

import jsonschema
import pytest

from opensmith_ingest.builder import MAX_DIFFICULTY, BeatGrid, build_chart
from opensmith_ingest.chart import Chart, validate
from opensmith_ingest.fingering import Note, Tuning
from opensmith_ingest.harmony import chord_name

BPM = 120.0
BEAT = 60.0 / BPM


@pytest.mark.parametrize(
    "pitches,name",
    [
        ([40, 47, 52, 56, 59, 64], "E"),
        ([40, 47, 52, 55, 59, 64], "Em"),
        ([45, 52, 57, 61, 64], "A"),
        ([45, 52, 57, 60, 64], "Am"),
        ([50, 57, 62, 66], "D"),
        ([43, 47, 50, 55, 59, 67], "G"),
        ([48, 52, 55, 60, 64], "C"),
        ([41, 48, 53, 57, 60, 65], "F"),
        ([47, 54, 59, 62, 66], "Bm"),
        ([40, 47, 52], "E5"),
        ([52, 55, 60, 64], "C/E"),
        ([47, 51, 57, 59, 66], "B7"),
        ([50, 57, 62, 67], "Dsus4"),
        ([48, 52, 55, 59, 64], "Cmaj7"),
        ([45, 52, 55, 60, 64], "Am7"),
        ([47, 50, 55, 59, 67], "G/B"),
        ([42, 50, 57, 62, 66], "D/F#"),
    ],
)
def test_chord_names(pitches, name):
    assert chord_name(pitches) == name


def riff_notes(bars=8, jitter=0.0, seed=0):
    """Two-bar riff repeated: E5 chord on 1, eighth-note line, big open E chord on bar 2."""
    rng = random.Random(seed)
    out = []
    for bar in range(0, bars, 2):
        t0 = 0.5 + bar * 4 * BEAT
        j = lambda: rng.uniform(-jitter, jitter)  # noqa: E731
        out += [Note(t0 + j(), p, 1.5 * BEAT) for p in (40, 47, 52)]
        for k, p in enumerate([55, 57, 55, 52]):
            out.append(Note(t0 + (2 + k * 0.5) * BEAT + j(), p, 0.4 * BEAT))
        t1 = t0 + 4 * BEAT
        out += [Note(t1 + j(), p, 2 * BEAT) for p in (40, 47, 52, 56, 59, 64)]
        out += [Note(t1 + 2.75 * BEAT + j(), 59, 0.2 * BEAT), Note(t1 + 3 * BEAT + j(), 57, 0.9 * BEAT)]
    return out


def make_chart(**kw):
    grid = BeatGrid.regular(BPM, 17.0, start=0.5)
    return build_chart(riff_notes(**kw), grid, title="Riff", artist="Test", duration=17.0)


def test_onsets_snap_to_the_beat_grid():
    chart = make_chart(jitter=0.02, seed=3)
    grid_points = {round(0.5 + k * BEAT / 12, 4) for k in range(400)}
    for n in chart.notes:
        assert round(n.time, 4) in grid_points
    for c in chart.chords:
        assert round(c.time, 4) in grid_points


def test_full_level_contains_every_note_once():
    chart = make_chart()
    notes, chords = chart.events_at(MAX_DIFFICULTY)
    played = sorted((round(n.time, 3), chart.tuning.pitch(n.string, n.fret)) for n in notes)
    for c in chords:
        frets = chart.templates[c.template].frets
        played += [(round(c.time, 3), chart.tuning.pitch(s, f)) for s, f in enumerate(frets) if f >= 0]
    expected = sorted((round(n.time, 3), n.pitch) for n in riff_notes())
    assert sorted(played) == expected


def test_levels_grow_from_downbeats_to_everything():
    chart = make_chart()
    counts = []
    for level in range(MAX_DIFFICULTY + 1):
        notes, chords = chart.events_at(level)
        counts.append(len(notes) + len(chords))
        if level == 0:
            for e in notes + chords:
                assert chart.beats  # events at level 0 sit on bar starts or phrase entries
                bars = {round(b.time, 4) for b in chart.beats if b.bar}
                assert round(e.time, 4) in bars
    assert counts == sorted(counts) and counts[0] < counts[-1], counts
    for e in chart.notes + chart.chords:
        assert 0 <= e.levels[0] <= e.levels[1] <= MAX_DIFFICULTY


def test_big_chords_are_simplified_at_low_levels():
    chart = make_chart()
    t = 0.5 + 4 * BEAT  # open E chord on bar 2, beat 1
    shapes = {}
    for level in range(MAX_DIFFICULTY + 1):
        notes, chords = chart.events_at(level)
        here = [n for n in notes if abs(n.time - t) < 1e-6] + [c for c in chords if abs(c.time - t) < 1e-6]
        assert len(here) == 1, (level, here)
        e = here[0]
        shapes[level] = chart.templates[e.template].name if hasattr(e, "template") else f"note {e.string}/{e.fret}"
    assert shapes[0] == shapes[1] == "note 0/0"
    assert shapes[2] == "E5"
    assert shapes[3] == shapes[4] == "E"


def test_every_phrase_has_something_at_level_zero_and_repeats_share_a_name():
    chart = make_chart()
    notes, chords = chart.events_at(0)
    for p in chart.phrases:
        if p.name == "rest":
            continue
        assert any(p.start <= e.time < p.end for e in notes + chords), p
    played = [p.name for p in chart.phrases if p.name != "rest"]
    assert played == ["A", "A"], played
    assert all(p.max_difficulty <= MAX_DIFFICULTY for p in chart.phrases)


def test_open_e_template_has_names_and_fingers():
    chart = make_chart()
    e = next(t for t in chart.templates if t.name == "E")
    assert e.frets == [0, 2, 2, 1, 0, 0]
    assert e.fingers == [0, 2, 3, 1, 0, 0]


def test_anchors_frame_every_fretted_note():
    chart = make_chart()
    for n in chart.notes:
        if n.fret == 0:
            continue
        active = [a for a in chart.anchors if a.time <= n.time + 1e-6][-1]
        assert active.fret <= n.fret <= active.fret + active.width - 1, (n, active)


def test_chart_validates_and_round_trips(tmp_path):
    chart = make_chart()
    path = tmp_path / "song_chart.json"
    doc = chart.save(path)
    again = Chart.load(path)
    assert again.to_dict() == doc
    bad = json.loads(path.read_text())
    bad["notes"][0]["levels"] = [0]
    with pytest.raises(jsonschema.ValidationError):
        validate(bad)


def test_rhythm_vs_lead_arrangement():
    assert make_chart().arrangement_type == "lead"
    grid = BeatGrid.regular(BPM, 8.0, start=0.0)
    strums = [Note(k * BEAT, p, BEAT) for k in range(8) for p in (40, 47, 52, 56, 59, 64)]
    chart = build_chart(strums, grid, title="Strum", artist="Test", duration=8.0)
    assert chart.arrangement_type == "rhythm"
    assert chart.hand_shapes and chart.hand_shapes[0].end > chart.hand_shapes[0].start


def test_drop_d_tuning_is_carried():
    grid = BeatGrid.regular(BPM, 4.0)
    drop_d = Tuning((38, 45, 50, 55, 59, 64), "Drop D")
    chart = build_chart([Note(0.0, 38, 1.0)], grid, title="D", artist="T", duration=4.0, tuning=drop_d)
    assert chart.to_dict()["arrangement"]["tuning"] == {"name": "Drop D", "strings": [38, 45, 50, 55, 59, 64]}
    assert (chart.notes[0].string, chart.notes[0].fret) == (0, 0)


def test_barre_and_same_fret_fingerings():
    grid = BeatGrid.regular(BPM, 4.0)
    shapes = {}
    for name, pitches in {"F": [41, 48, 53, 57, 60, 65], "A": [45, 52, 57, 61, 64], "C": [48, 52, 55, 60, 64]}.items():
        chart = build_chart([Note(0.0, p, 2.0) for p in pitches], grid, title=name, artist="T", duration=4.0)
        # Lower levels reuse the chord's name on fewer strings: take the complete shape.
        full = max((t for t in chart.templates if t.name == name), key=lambda t: sum(f >= 0 for f in t.frets))
        shapes[name] = full.fingers
    assert shapes["F"] == [1, 3, 4, 2, 1, 1]
    # Three fingers side by side rather than one per fret from the nut: how most tabs write it.
    assert shapes["A"] == [-1, 0, 1, 2, 3, 0]
    assert shapes["C"] == [-1, 3, 2, 0, 1, 0]


def test_the_package_carries_the_repository_s_chart_schema():
    from pathlib import Path

    from opensmith_ingest import chart

    packaged = Path(chart.__file__).with_name("chart.schema.json")
    assert packaged.read_text() == chart.REPO_SCHEMA.read_text(), "copy schema/chart.schema.json into the package"
