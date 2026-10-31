"""The bench's measures, on charts written here (the pipeline itself is not run)."""

import json

import pytest

from opensmith_ingest.bench import Notes, SongResult, chart_notes, references, score, summary
from opensmith_ingest.chart import Chart, ChartChord, ChartNote, ChordTemplate
from opensmith_ingest.fingering import Tuning

RIFF = [(0.5, 0, 3), (1.0, 1, 2), (1.5, 2, 2), (2.0, 1, 5)]


def make(notes=RIFF, *, shift=0.0, octave=0, string_moved=False, kind="feedpak", cents=0.0, offset=0.0) -> Chart:
    chart = Chart("Night Drive", "The Larsens", 4.0, kind, "x", Tuning(cents=cents), max_difficulty=1)
    chart.audio_offset = offset
    for t, s, f in notes:
        if string_moved and s == 1:
            s, f = 0, f + 5  # same pitch, another string
        chart.notes.append(ChartNote(t + shift, s, f + 12 * octave, 0.2, (0, 1)))
    chart.templates = [ChordTemplate("E5", [0, 2, 2, -1, -1, -1], [0, 1, 1, -1, -1, -1])]
    chart.chords.append(ChartChord(3.0 + shift, 0, 0.5, (1, 1)))
    return chart


def test_a_chart_is_read_as_its_notes_at_the_top_level_with_chords_spread():
    notes = chart_notes(make(cents=30.0, offset=0.25))
    assert len(notes) == 4 + 3
    assert notes.intervals[0, 0] == 0.75, "chart times moved by the audio offset"
    assert notes.pitches[0] == pytest.approx(43.3), "low E string, fret 3, the recording 30 cents up"
    assert notes.places[-3:] == [(0, 0), (1, 2), (2, 2)]


def test_the_same_notes_score_everything():
    s, matched = score(chart_notes(make()), chart_notes(make()))
    assert (s.reference, s.found, s.matched) == (7, 7, 7)
    assert s.f1() == 1.0 and s.lag_ms == 0.0 and len(matched) == 7


def test_octave_errors_and_late_notes_are_told_apart():
    ref = chart_notes(make())
    up = score(ref, chart_notes(make(octave=1)))[0]
    assert up.matched == 3 and up.matched_any_octave == 7, "only the chord (not moved) is exact"
    late = score(ref, chart_notes(make(shift=0.03)))[0]
    assert late.matched == 7 and late.lag_ms == pytest.approx(30.0)
    too_late = score(ref, chart_notes(make(shift=0.08)))[0]
    assert too_late.matched == 0 and too_late.matched_onset == 0 and too_late.lag_ms == pytest.approx(80.0)


def test_fingering_is_compared_on_matched_notes():
    ref, est = chart_notes(make()), chart_notes(make(string_moved=True))
    s, matched = score(ref, est)
    assert s.matched == 7
    same = sum(ref.places[i] == est.places[j] for i, j in matched)
    assert same == 5, "the two notes moved to the low string"


def test_transcriptions_without_strings_are_scored_too():
    raw = Notes.of([(0.51, 0.7, 43.0, None), (1.0, 1.2, 47.0, None), (2.5, 2.6, 80.0, None)])
    s, _ = score(chart_notes(make()), raw)
    assert (s.matched, s.found, s.reference) == (2, 3, 7)


def test_only_hand_written_guitar_charts_with_audio_are_references(tmp_path):
    def song(name, kind="feedpak", kind_of_part="lead", audio=True):
        folder = tmp_path / name
        folder.mkdir()
        chart = make(kind=kind)
        chart.arrangement_type = kind_of_part
        chart.audio_backing = "song.wav" if audio else None
        chart.save(folder / "song_chart.json")
        if audio:
            (folder / "song.wav").write_bytes(b"RIFF")

    for k in range(6):
        song(f"hand{k}")
    song("transcribed", kind="audio")
    song("demo", kind="synth")
    song("bass", kind_of_part="bass")
    song("tab-only", audio=False)
    (tmp_path / "broken").mkdir()
    (tmp_path / "broken" / "song_chart.json").write_text("{", encoding="utf-8")

    found = references(tmp_path)
    assert sorted(p.name for p in found) == [f"hand{k}" for k in range(6)]
    assert references(tmp_path, 3, seed=7) == references(tmp_path, 3, seed=7), "the same seed draws the same songs"
    assert len(references(tmp_path, 3)) == 3


def test_the_summary_adds_songs_up(tmp_path):
    ref = chart_notes(make())
    a = SongResult("a", "Night Drive", "The Larsens", "Lead", "E Standard", "E Standard")
    a.stages = {"raw": score(ref, chart_notes(make()))[0], "chart": score(ref, chart_notes(make(octave=1)))[0]}
    a.fingering_matched, a.fingering_same = 7, 7
    b = SongResult("b", "Backstage", "The Larsens", "Lead", "Drop D", "E Standard (written: Drop D)", error=None)
    b.stages = {"raw": score(ref, chart_notes(make(shift=0.08)))[0], "chart": score(ref, chart_notes(make()))[0]}
    failed = SongResult("c", "c", "", "", "", error="RuntimeError: ffmpeg")
    text = summary([a, b, failed])
    assert text.splitlines()[0] == "2 songs measured, 1 failed"
    raw = next(line for line in text.splitlines() if line.startswith("raw"))
    assert "50.0%" in raw, "7 of 14 reference notes found over both songs"
    assert "tuning found: 1/2" in text
    # A result survives its cache file.
    again = SongResult.from_json(json.loads(json.dumps(a.to_json())))
    assert again.stages["chart"].matched == a.stages["chart"].matched


def result(name, chart_shift=0.0):
    ref = chart_notes(make())
    r = SongResult(name, name.title(), "The Larsens", "Lead", "E Standard", "E Standard", seconds=40.0)
    r.stages = {"raw": score(ref, chart_notes(make()))[0], "chart": score(ref, chart_notes(make(shift=chart_shift)))[0]}
    return r


def test_runs_are_recorded_for_the_dashboard(tmp_path):
    import urllib.request
    from threading import Thread

    from opensmith_ingest.bench import Run
    from opensmith_ingest.bench_view import server, state

    # Before any run: the songs in the cache are shown as one.
    (tmp_path / "old").mkdir()
    (tmp_path / "old" / "bench.json").write_text(json.dumps(result("old").to_json()), encoding="utf-8")
    assert state(tmp_path)["latest"]["summary"]["songs"] == 1

    first = Run(tmp_path, 2, {"limit": 2}, "baseline")
    first.progress("night")
    assert state(tmp_path)["progress"]["current"] == "night" and state(tmp_path)["progress"]["done"] == 0
    first.add(result("night", chart_shift=0.08))
    seen = state(tmp_path)
    assert seen["progress"]["done"] == 1 and seen["progress"]["current"] is None and seen["progress"]["finished"] is None
    assert seen["latest"]["label"] == "baseline", "a run under way already counts"
    first.add(result("backstage"))
    first.save(finished=True)
    first.progress(None, finished=True)

    second = Run(tmp_path, 2, {"limit": 2, "rechart": True}, "better onsets")
    second.id = first.id + "b"  # two runs in the same second
    for name in ("night", "backstage"):
        second.add(result(name))
    second.save(finished=True)

    seen = state(tmp_path)
    assert [r["label"] for r in seen["runs"]] == ["baseline", "better onsets"]
    assert seen["runs"][0]["summary"]["stages"]["chart"]["f1"] == pytest.approx(0.5)
    assert seen["runs"][1]["summary"]["stages"]["chart"]["f1"] == 1.0
    assert seen["previous"]["label"] == "baseline" and len(seen["latest"]["songs"]) == 2

    httpd = server(tmp_path, 0)
    Thread(target=httpd.serve_forever, daemon=True).start()
    try:
        base = f"http://127.0.0.1:{httpd.server_address[1]}"
        page = urllib.request.urlopen(base + "/").read().decode("utf-8")
        assert "Banc d'essai" in page
        api = json.loads(urllib.request.urlopen(base + "/api/state").read())
        assert api["latest"]["label"] == "better onsets"
    finally:
        httpd.shutdown()
