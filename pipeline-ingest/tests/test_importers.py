"""Guitar Pro importer."""

import guitarpro as gp
import pytest

from opensmith_ingest.importers.guitarpro import load_guitarpro, unroll

def make_gp5(path, markers=None):
    """4/4 at 100 BPM: bars 1-2 repeated twice, bar 3 at 120 BPM. `markers`: {bar index: title}."""
    song = gp.Song()
    song.title, song.artist, song.tempo = "GP Test", "Tab Author", 100
    headers = []
    for k in range(3):
        h = gp.MeasureHeader(number=k + 1, start=gp.Duration.quarterTime * (1 + 4 * k))
        if k in (markers or {}):
            h.marker = gp.Marker(title=markers[k])
        headers.append(h)
    headers[0].isRepeatOpen = True
    headers[1].repeatClose = 1
    song.measureHeaders = headers
    track = song.tracks[0]
    track.name = "Lead Guitar"
    track.measures = [gp.Measure(track, h) for h in headers]

    def beat(measure, value, notes, effect=None):
        voice = measure.voices[0]
        b = gp.Beat(voice, duration=gp.Duration(value=value), status=gp.BeatStatus.normal)
        if effect is not None:
            b.effect = effect
        for string, fret, kind, neffect in notes:
            b.notes.append(gp.Note(b, value=fret, string=string, type=kind, effect=neffect or gp.NoteEffect()))
        voice.beats.append(b)

    normal, tie = gp.NoteType.normal, gp.NoteType.tie
    m1, m2, m3 = track.measures
    beat(m1, 4, [(6, 0, normal, None), (5, 2, normal, None)])  # E5
    beat(m1, 4, [(4, 2, normal, None)])  # E3
    beat(m1, 4, [(4, 4, normal, gp.NoteEffect(hammer=True))])  # F#3, hammer-on
    beat(m1, 4, [(3, 2, normal, gp.NoteEffect(slides=[gp.SlideType.shiftSlideTo]))])  # A3 slide…
    beat(m2, 2, [(3, 4, normal, None)])  # …to B3, tied over
    beat(m2, 4, [(3, 4, tie, None)])
    beat(m2, 4, [(2, 2, normal, gp.NoteEffect(palmMute=True))])
    tempo = gp.BeatEffect(mixTableChange=gp.MixTableChange(tempo=gp.MixTableItem(value=120)))
    beat(m3, 1, [(6, 0, normal, None), (5, 2, normal, None), (4, 2, normal, None)], effect=tempo)
    gp.write(song, str(path))


def test_repeats_and_alternate_endings_unroll():
    hs = [gp.MeasureHeader(number=k + 1) for k in range(5)]
    hs[0].isRepeatOpen = True
    hs[2].repeatAlternative = 1
    hs[2].repeatClose = 1
    hs[3].repeatAlternative = 2
    assert unroll(hs) == [0, 1, 2, 0, 1, 3, 4]


def test_guitar_pro_file_becomes_a_chart(tmp_path):
    path = tmp_path / "test.gp5"
    make_gp5(path)
    c = load_guitarpro(path)
    beat = 0.6
    assert (c.title, c.artist, c.arrangement_name) == ("GP Test", "Tab Author", "Lead Guitar")
    assert c.tuning.strings == (40, 45, 50, 55, 59, 64)
    # bars 1, 2, 1, 2 at 100 BPM, then bar 3 at 120 BPM
    bars = [b.time for b in c.beats if b.bar]
    assert bars == pytest.approx([0.0, 4 * beat, 8 * beat, 12 * beat, 16 * beat])
    assert c.duration == pytest.approx(16 * beat + 4 * 0.5)
    full_notes, full_chords = c.events_at(c.max_difficulty)
    e5 = [ch for ch in full_chords if c.templates[ch.template].name == "E5"]
    # Bar 1 twice (repeat), then bar 3's E2-B2-E3, also an E5.
    assert [round(ch.time, 3) for ch in e5] == [0.0, round(8 * beat, 3), round(16 * beat, 3)]
    (hammer,) = [n for n in full_notes if abs(n.time - beat * 2) < 1e-6]
    assert (hammer.string, hammer.fret, hammer.techniques) == (2, 4, {"hammerOn": True})
    (slide,) = [n for n in full_notes if abs(n.time - beat * 3) < 1e-6]
    assert slide.techniques == {"slideTo": 4}
    (tied,) = [n for n in full_notes if abs(n.time - beat * 4) < 1e-6]
    assert (tied.string, tied.fret) == (3, 4) and tied.sustain == pytest.approx(3 * beat)
    (muted,) = [n for n in full_notes if abs(n.time - beat * 7) < 1e-6]
    assert muted.techniques == {"palmMute": True}
    last = [ch for ch in full_chords if ch.time > 15 * beat]
    assert len(last) == 1 and c.templates[last[0].template].frets[:3] == [0, 2, 2]
    c.save(tmp_path / "chart.json")


def test_guitar_pro_markers_name_the_sections(tmp_path):
    path = tmp_path / "marked.gp5"
    make_gp5(path, markers={1: "Verse", 2: "Chorus"})
    c = load_guitarpro(path)
    bar = 4 * 0.6
    # Bars 1, 2, 1, 2, 3: "Verse" heads bar 2, played twice in a row by the repeat; bar 1,
    # played first, is the intro.
    assert [s.name for s in c.sections] == ["Intro", "Verse", "Chorus"]
    assert [s.start for s in c.sections] == pytest.approx([0.0, bar, 4 * bar])
    assert c.sections[-1].end == pytest.approx(c.duration)
    # Phrases start again with each section.
    assert [p.start for p in c.phrases] == pytest.approx([0.0, bar, 3 * bar, 4 * bar])
