"""A Guitar Pro tab placed on a recording: names, timing under tempo drift, wrong tabs."""

import numpy as np
import pytest

from opensmith_ingest.align import find_tab, place
from opensmith_ingest.fingering import Fingered, Note, Tuning
from opensmith_ingest.importers.guitarpro import TabPart
from opensmith_ingest.stem import RATE, GuitarStem
from opensmith_ingest.synth import _string, midi_hz
from opensmith_ingest.transcription import RawNote

D_STANDARD = Tuning((38, 43, 48, 53, 57, 62), "D Standard")
RIFF = [38, 39, 40, 43, 40, 38, 45, 43, 40, 39, 38, 43, 38, 38]  # a single-note riff in D standard
RHYTHM = [0, 0.5, 1, 1.5, 2.5, 3, 3.5, 4, 4.5, 5, 5.5, 6, 6.5, 7]  # beats in a 2-bar riff


def tab_part(pitches=RIFF, bars=16, bpm=120.0) -> TabPart:
    beat = 60 / bpm
    tab = []
    for rep in range(bars // 2):
        for p, b in zip(pitches, RHYTHM):
            t = (rep * 8 + b) * beat
            tab.append(Fingered(t, [Note(t, p, 0.45 * beat)], [D_STANDARD.positions(p)[0]], 1))
    beats = [k * beat for k in range(bars * 4)]
    return TabPart(tab, beats, list(range(0, bars * 4, 4)), 4, bars * 4 * beat, D_STANDARD, "Song", "Band", None, "Guitar")


def song_time(t: float) -> float:
    """The band: a 3 s intro the tab does not have, 6 % slower, and a slow drift."""
    return 3.0 + 1.06 * t + 0.12 * np.sin(2 * np.pi * t / 17)


def record(part: TabPart, seed: int = 0) -> tuple[GuitarStem, list[RawNote], np.ndarray]:
    rng = np.random.default_rng(seed)
    times = np.array([song_time(sl.time) for sl in part.tab])
    y = np.zeros(int((times[-1] + 2) * RATE))
    for sl, t in zip(part.tab, times):
        sig = 0.3 * _string(midi_hz(sl.notes[0].pitch), 0.4, RATE, rng, t60=1.0)
        i = int(t * RATE)
        y[i : i + sig.size] += sig[: max(0, min(sig.size, y.size - i))]
    y += 1e-3 * rng.standard_normal(y.size)
    # What a transcription makes of it: 60 % of the notes, a third of them an octave up.
    raw = []
    for sl, t in zip(part.tab, times):
        if rng.random() < 0.6:
            p = sl.notes[0].pitch + (12 if rng.random() < 0.33 else 0)
            raw.append(RawNote(float(t + rng.normal(0, 0.008)), float(t + 0.2), p, 0.5))
    return GuitarStem(y), raw, times


def test_notes_land_on_the_recording_despite_intro_tempo_and_drift():
    part = tab_part()
    stem, raw, truth = record(part)
    placed = place(part, stem, raw)
    error = np.abs(np.array([sl.time for sl in placed.tab]) - truth)
    assert np.median(error) < 0.02
    assert np.mean(error < 0.05) > 0.9
    assert placed.confirmed > 0.45  # the transcription heard 60 % of the notes
    assert placed.start == pytest.approx(3.0, abs=0.1)
    # The tab's bars become the song's bars.
    assert placed.grid.beats[4] == pytest.approx(song_time(part.beats[4]), abs=0.08)


def test_a_tab_of_another_song_is_not_confirmed():
    part = tab_part()
    stem, raw, _ = record(part)
    other = tab_part(pitches=[50, 53, 55, 57, 60, 57, 55, 53, 50, 48, 46, 45, 48, 50])
    assert place(other, stem, raw).confirmed < 0.1


def test_tabs_are_found_by_the_song_name(tmp_path):
    for name in [
        "Larsen - Come Back Home (Official Music Video).mp3",
        "Band Of The Week - D.I.Y. (Official HD Video).mp3",
        "Band Of The Week - Night Drive (Official HD Video).mp3",
        "larsen-come_back_home.gp3",
        "band-of-the-week-d_i_y.gp4",
        "notes.gp5",
    ]:
        (tmp_path / name).write_bytes(b"")
    assert find_tab(tmp_path / "Larsen - Come Back Home (Official Music Video).mp3").name == "larsen-come_back_home.gp3"
    assert find_tab(tmp_path / "Band Of The Week - D.I.Y. (Official HD Video).mp3").name == "band-of-the-week-d_i_y.gp4"
    assert find_tab(tmp_path / "Band Of The Week - Night Drive (Official HD Video).mp3") is None


def write_riff_gp5(path, pitches: list[int], bars: int, tempo: int = 120) -> None:
    """A Guitar Pro 5 file in D standard: `pitches` as eighth notes, over and over."""
    import guitarpro as gp

    song = gp.Song()
    song.title, song.artist, song.tempo = "Song", "Band", tempo
    song.measureHeaders = [gp.MeasureHeader(number=k + 1, start=gp.Duration.quarterTime * (1 + 4 * k)) for k in range(bars)]
    track = song.tracks[0]
    track.name = "Guitar"
    track.strings = [gp.GuitarString(k + 1, v) for k, v in enumerate(reversed(D_STANDARD.strings))]
    track.measures = [gp.Measure(track, h) for h in song.measureHeaders]
    for k, measure in enumerate(track.measures):
        voice = measure.voices[0]
        for j in range(8):
            pitch = pitches[(8 * k + j) % len(pitches)]
            string, fret = D_STANDARD.positions(pitch)[0]
            b = gp.Beat(voice, duration=gp.Duration(value=8), status=gp.BeatStatus.normal)
            b.notes.append(gp.Note(b, value=fret, string=6 - string, type=gp.NoteType.normal))
            voice.beats.append(b)
    gp.write(song, str(path))


def test_a_tab_goes_onto_an_imported_song(tmp_path):
    from dataclasses import asdict

    from opensmith_ingest import audio_io
    from opensmith_ingest.chart import Chart
    from opensmith_ingest.importers.guitarpro import load_guitarpro, parse_guitarpro
    from opensmith_ingest.pipeline import retab

    riff = RIFF + [38, 38]
    tab = tmp_path / "band-song.gp5"
    write_riff_gp5(tab, riff, bars=16)
    stem, raw, truth = record(parse_guitarpro(tab))
    # An imported song: stems, cached transcription, a chart (here the tab's, at its own tempo).
    folder = tmp_path / "Band - Song"
    folder.mkdir()
    audio_io.write_wav(folder / "isolated_guitar.wav", stem.y[None, :], RATE)
    audio_io.write_wav(folder / "backing_track_no_guitar.wav", np.zeros((1, stem.y.size), dtype=np.float32), RATE)
    (folder / "transcription.json").write_text(__import__("json").dumps([asdict(n) for n in raw]))
    old = load_guitarpro(tab)
    old.audio_guitar, old.audio_backing, old.duration = "isolated_guitar.wav", "backing_track_no_guitar.wav", stem.y.size / RATE
    old.save(folder / "song_chart.json")

    report = retab(folder, tab)
    chart = Chart.load(folder / "song_chart.json")
    first = min(n.time for n in chart.notes)
    assert first == pytest.approx(truth[0], abs=0.05)
    assert chart.source_kind == "guitarpro" and chart.audio_guitar == "isolated_guitar.wav"
    assert report.confirmed > 0.45
    assert (folder / "song_chart.previous.json").is_file()
    assert __import__("json").loads((folder / "tab.json").read_text())["source"] == str(tab.resolve())

    other = tmp_path / "other.gp5"
    write_riff_gp5(other, [50, 53, 55, 57, 60, 57, 55, 53], bars=16)
    with pytest.raises(ValueError, match="ne correspond pas"):
        retab(folder, other)
