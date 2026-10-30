"""Guitar Pro 3/4/5 → chart (PyGuitarPro). The tab's own fingering is kept.

Repeats and alternate endings are unrolled into the performed timeline, tempo changes are
followed, tied notes extend their note, and techniques (hammer-on/pull-off, slides, bends,
vibrato, palm mute, dead notes, harmonics, accents, let ring) are carried over. The tab's
markers (Verse, Chorus, Solo…) become the chart's sections; difficulty levels and phrases are
generated like for transcriptions.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

import guitarpro as gp

from ..builder import BeatGrid, build_chart
from ..chart import Chart
from ..fingering import Fingered, Note, Tuning, infer_anchors

TICKS_PER_QUARTER = gp.Duration.quarterTime
NAMED_TUNINGS = {
    (40, 45, 50, 55, 59, 64): "E Standard",
    (38, 45, 50, 55, 59, 64): "Drop D",
    (39, 44, 49, 54, 58, 63): "Eb Standard",
    (37, 44, 49, 54, 58, 63): "Drop C#",
    (36, 43, 48, 53, 57, 62): "Drop C",
    (38, 43, 48, 53, 57, 62): "D Standard",
}


def unroll(headers: list[gp.MeasureHeader]) -> list[int]:
    """Measure indices in playing order: repeats taken, alternate endings on their pass."""
    order: list[int] = []
    group_start, pass_no = 0, 1
    jumps: dict[int, int] = {}
    i = 0
    while i < len(headers) and len(order) < 100_000:
        h = headers[i]
        if h.isRepeatOpen and i != group_start:
            group_start, pass_no = i, 1
        if h.repeatAlternative and not (h.repeatAlternative >> (pass_no - 1)) & 1:
            i += 1
            continue
        order.append(i)
        if h.repeatClose > 0:
            done = jumps.get(i, 0)
            if done < h.repeatClose:
                jumps[i] = done + 1
                pass_no += 1
                i = group_start
                continue
            jumps[i] = 0
        i += 1
    return order


def _pick_track(song: gp.Song, track: int | None) -> gp.Track:
    if track is not None:
        return song.tracks[track]
    for t in song.tracks:
        if not t.isPercussionTrack and len(t.strings) >= 6:
            return t
    for t in song.tracks:
        if not t.isPercussionTrack:
            return t
    raise ValueError("no pitched track in this file")


def _techniques(note: gp.Note) -> dict:
    e = note.effect
    out: dict = {}
    if e.hammer:
        out["hopo"] = True  # resolved to hammerOn / pullOff once the previous note is known
    if e.bend and e.bend.points:
        amount = max(p.value for p in e.bend.points) / gp.BendEffect.semitoneLength
        if amount > 0:
            out["bend"] = float(amount)
    if e.vibrato or note.beat.effect.vibrato:
        out["vibrato"] = True
    if e.palmMute:
        out["palmMute"] = True
    if note.type == gp.NoteType.dead:
        out["mute"] = True
    if e.harmonic is not None:
        out["pinchHarmonic" if isinstance(e.harmonic, gp.PinchHarmonic) else "harmonic"] = True
    if e.accentuatedNote or e.heavyAccentuatedNote:
        out["accent"] = True
    if any(s in (gp.SlideType.shiftSlideTo, gp.SlideType.legatoSlideTo) for s in e.slides):
        out["slide"] = True  # resolved to slideTo once the next note on the string is known
    return out


@dataclass
class TabPart:
    """One track of a tab, at the tab's own tempo: fingered slices, beats (s) with the index of
    each bar's first beat, and what the file says about the song."""

    tab: list[Fingered]
    beats: list[float]
    bars: list[int]
    beats_per_bar: int
    duration: float
    tuning: Tuning
    title: str
    artist: str
    album: str | None
    track_name: str | None
    # The tab's markers: (index in `bars` of the bar they head, title), in playing order.
    marks: list[tuple[int, str]] = field(default_factory=list)


def named_sections(part: TabPart, grid: BeatGrid) -> list[tuple[float, str]]:
    """The tab's markers at the time of their bar on `grid` (the tab's own or placed on a
    recording)."""
    bar_times = grid.bar_times()
    return [(bar_times[i], title) for i, title in part.marks if i < len(bar_times)]


def load_guitarpro(path: Path, track: int | None = None) -> Chart:
    path = Path(path)
    part = parse_guitarpro(path, track)
    grid = BeatGrid(part.beats + [part.duration], part.bars, part.beats_per_bar)
    chart = build_chart(
        None,
        grid,
        title=part.title or path.stem,
        artist=part.artist or "Unknown",
        duration=part.duration,
        tuning=part.tuning,
        source_kind="guitarpro",
        source_file=path.name,
        arrangement_name=part.track_name,
        tab=part.tab,
        sections=named_sections(part, grid),
    )
    chart.album = part.album
    return chart


def parse_guitarpro(path: Path, track: int | None = None) -> TabPart:
    """The notes of `track` (default: the first guitar) in playing order, at the tab's tempo."""
    path = Path(path)
    song = gp.parse(str(path))
    tr = _pick_track(song, track)
    strings = sorted(tr.strings, key=lambda s: s.number, reverse=True)  # lowest first
    pitches = tuple(s.value for s in strings)
    tuning = Tuning(pitches, NAMED_TUNINGS.get(pitches, "Custom"), tr.offset)
    n_strings = len(strings)

    tempo_changes: dict[int, list[tuple[int, float]]] = {}
    for t in song.tracks:
        for idx, m in enumerate(t.measures):
            for v in m.voices:
                for b in v.beats:
                    mix = b.effect.mixTableChange
                    if mix is not None and mix.tempo is not None and mix.tempo.value > 0:
                        tempo_changes.setdefault(idx, []).append((b.start - m.header.start, float(mix.tempo.value)))

    bpm = float(song.tempo)
    clock = 0.0
    beats: list[float] = []
    bars: list[int] = []
    marks: list[tuple[int, str]] = []
    # (time, string, fret, duration, techniques) in playing order
    events: list[list] = []
    sounding: dict[int, list] = {}
    for idx in unroll(song.measureHeaders):
        header = song.measureHeaders[idx]
        num, den = header.timeSignature.numerator, header.timeSignature.denominator.value
        length = num * TICKS_PER_QUARTER * 4 // den
        changes = sorted(tempo_changes.get(idx, []))
        start_bpm = bpm

        def seconds(offset: int) -> float:
            t, last, tempo = 0.0, 0, start_bpm
            for at, value in changes:
                if at >= offset:
                    break
                t += (at - last) / TICKS_PER_QUARTER * 60.0 / tempo
                last, tempo = at, value
            return t + (offset - last) / TICKS_PER_QUARTER * 60.0 / tempo

        if header.marker is not None and header.marker.title.strip():
            marks.append((len(bars), header.marker.title.strip()))
        bars.append(len(beats))
        beats += [clock + seconds(k * TICKS_PER_QUARTER * 4 // den) for k in range(num)]
        measure = tr.measures[idx]
        for voice in measure.voices:
            for beat in voice.beats:
                offset = beat.start - header.start
                onset = clock + seconds(offset)
                dur = seconds(offset + beat.duration.time) - seconds(offset)
                for note in beat.notes:
                    if note.type == gp.NoteType.rest:
                        continue
                    s = n_strings - note.string
                    if note.type == gp.NoteType.tie and s in sounding:
                        sounding[s][3] += dur
                        continue
                    ev = [onset, s, note.value, dur, _techniques(note)]
                    events.append(ev)
                    sounding[s] = ev
        for at, value in changes:
            bpm = value
        clock += seconds(length)

    # Resolve techniques that depend on neighbouring notes on the same string.
    events.sort(key=lambda e: (e[0], e[1]))
    last_on_string: dict[int, list] = {}
    for ev in events:
        s = ev[1]
        prev = last_on_string.get(s)
        if prev is not None:
            if prev[4].pop("slide", False):
                prev[4]["slideTo"] = ev[2]
            if ev[4].pop("hopo", False):
                ev[4]["hammerOn" if ev[2] > prev[2] else "pullOff"] = True
        ev[4].pop("hopo", None)
        last_on_string[s] = ev
    for ev in events:
        ev[4].pop("slide", None)

    slices: dict[float, list] = {}
    for ev in events:
        slices.setdefault(round(ev[0], 6), []).append(ev)
    tab = []
    for t in sorted(slices):
        evs = sorted(slices[t], key=lambda e: tuning.pitch(e[1], e[2]))
        notes = [Note(t, tuning.pitch(e[1], e[2]), e[3]) for e in evs]
        tab.append(Fingered(t, notes, [(e[1], e[2]) for e in evs], 1, [e[4] for e in evs]))
    infer_anchors(tab, tuning)

    beats_per_bar = song.measureHeaders[0].timeSignature.numerator if song.measureHeaders else 4
    return TabPart(tab, beats, bars, beats_per_bar, clock, tuning, song.title or "", song.artist or "", song.album or None, tr.name or None, marks)
