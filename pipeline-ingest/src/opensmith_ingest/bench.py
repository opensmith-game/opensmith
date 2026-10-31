"""How well the MP3 pipeline finds a song's notes, measured against charts written by hand.

Songs of a library whose chart was written by a person (a feedpak, a plugin's import: any chart
not transcribed by the pipeline, with its own audio) go through the pipeline from that audio,
exactly as an MP3 would, and what it finds is compared with what was written, stage by stage:

    mix     Basic Pitch on the whole mix, without separation (with --mix): what separation gains
    raw     Basic Pitch on the separated guitar stem
    clean   the notes kept after the stem's checks (attacks, pitch, level)
    chart   the playable chart, every level (quantized, voted across repeats, fingered)

Two notes match when their onsets are within 50 ms and their pitches within half a semitone
(mir_eval). Also measured: the same ignoring octaves and ignoring pitch (timing alone), the
median lag of onsets, the fingering of matched chart notes (same string and fret) and whether
the tuning was found.

Work files go to a cache folder, one per song: the chart, Basic Pitch's notes and the guitar
stem (mono FLAC, a few MB), so a later run measures again without separating, and `rechart`
rebuilds the charts from them with the current rules. The results only describe the songs
measured: nothing of them is meant to leave the machine.
"""

from __future__ import annotations

import json
import random
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import mir_eval
import numpy as np
import soundfile as sf

from .chart import Chart

ONSET = 0.05
LAG_WINDOW = 0.1
MIN_LENGTH = 0.05
STEM = "isolated_guitar.flac"
STEM_RATE = 22_050
RESULT = "bench.json"
MIX_CACHE = "transcription_mix.json"
# Charts the pipeline made itself, or generated: no reference.
NOT_WRITTEN = ("audio", "synth")


@dataclass
class Notes:
    """Onsets and ends (s), fractional MIDI pitches and, for charts, (string, fret)."""

    intervals: np.ndarray
    pitches: np.ndarray
    places: list[tuple[int, int]] = field(default_factory=list)

    @classmethod
    def of(cls, rows: list[tuple[float, float, float, tuple[int, int] | None]]) -> Notes:
        rows = sorted(rows, key=lambda r: (r[0], r[2]))
        iv = np.array([[a, max(b, a + MIN_LENGTH)] for a, b, _, _ in rows], dtype=float).reshape(-1, 2)
        return cls(iv, np.array([p for _, _, p, _ in rows], dtype=float), [pl for *_, pl in rows if pl is not None])

    def __len__(self) -> int:
        return len(self.pitches)


def chart_notes(chart: Chart) -> Notes:
    """Every note of a chart at its top level, chords spread into their strings, at the times of
    its audio and the pitch of its recording (`tuning.cents`)."""
    notes, chords = chart.events_at(chart.max_difficulty)
    t0, strings, cents = chart.audio_offset, chart.tuning.strings, chart.tuning.cents / 100.0
    rows = [(n.time + t0, n.time + t0 + n.sustain, strings[n.string] + n.fret + cents, (n.string, n.fret)) for n in notes if n.string < len(strings)]
    for c in chords:
        shape = chart.templates[c.template].frets if c.template < len(chart.templates) else []
        for s, f in enumerate(shape[: len(strings)]):
            if f >= 0:
                rows.append((c.time + t0, c.time + t0 + c.sustain, strings[s] + f + cents, (s, f)))
    return Notes.of(rows)


def raw_notes(rows: list[dict]) -> Notes:
    return Notes.of([(float(n["start"]), float(n["end"]), float(n["pitch"]), None) for n in rows])


def _hz(midi: np.ndarray) -> np.ndarray:
    return 440.0 * 2.0 ** ((midi - 69.0) / 12.0)


@dataclass
class Score:
    """Counts, so that songs add up (micro average) besides their own F1."""

    reference: int = 0
    found: int = 0
    matched: int = 0
    matched_any_octave: int = 0
    matched_onset: int = 0
    lag_ms: float | None = None

    def f1(self, matched: int | None = None) -> float:
        m = self.matched if matched is None else matched
        return 2.0 * m / (self.reference + self.found) if self.reference + self.found else 0.0

    def add(self, other: Score) -> None:
        for k in ("reference", "found", "matched", "matched_any_octave", "matched_onset"):
            setattr(self, k, getattr(self, k) + getattr(other, k))


def _matches(ref: Notes, est: Notes, ref_p: np.ndarray, est_p: np.ndarray, onset: float = ONSET) -> list[tuple[int, int]]:
    if not len(ref) or not len(est):
        return []
    return mir_eval.transcription.match_notes(ref.intervals, _hz(ref_p), est.intervals, _hz(est_p), onset_tolerance=onset, offset_ratio=None)


def score(ref: Notes, est: Notes) -> tuple[Score, list[tuple[int, int]]]:
    """How many of `ref`'s notes `est` finds: exactly, at any octave, at the right time only."""
    exact = _matches(ref, est, ref.pitches, est.pitches)
    octave = _matches(ref, est, 60 + ref.pitches % 12, 60 + est.pitches % 12)
    onset = _matches(ref, est, np.full(len(ref), 60.0), np.full(len(est), 60.0))
    loose = _matches(ref, est, ref.pitches, est.pitches, LAG_WINDOW)
    lags = [est.intervals[j, 0] - ref.intervals[i, 0] for i, j in loose]
    s = Score(len(ref), len(est), len(exact), len(octave), len(onset), round(1000 * float(np.median(lags)), 1) if lags else None)
    return s, exact


@dataclass
class SongResult:
    song: str
    title: str
    artist: str
    arrangement: str
    tuning: str
    tuning_found: str | None = None
    stages: dict[str, Score] = field(default_factory=dict)
    fingering_matched: int = 0
    fingering_same: int = 0
    seconds: float = 0.0
    error: str | None = None
    # The clean-up with one of its steps off (see CLEAN_STEPS).
    ablation: dict[str, Score] = field(default_factory=dict)

    def to_json(self) -> dict:
        d = asdict(self)
        d["stages"] = {k: asdict(v) for k, v in self.stages.items()}
        d["ablation"] = {k: asdict(v) for k, v in self.ablation.items()}
        return d

    @classmethod
    def from_json(cls, d: dict) -> SongResult:
        d = dict(d)
        stages = {k: Score(**v) for k, v in d.pop("stages").items()}
        ablation = {k: Score(**v) for k, v in d.pop("ablation", {}).items()}
        return cls(**d, stages=stages, ablation=ablation)


# Each step of `transcription.clean`, turned off on its own: what it costs and what it saves.
CLEAN_STEPS = {
    "amplitude": {"min_amplitude": 0.0},
    "silence": {"silence": False},
    "attack": {"onsets": np.array([])},
    "ghosts": {"ghosts": False},
    "merge": {"merge_gap": -1.0},
    "consolidate": {"consolidated": False},
}


def references(library: Path, limit: int | None = None, seed: int = 1) -> list[Path]:
    """Folders of `library` holding a hand-written guitar chart with its audio, in a fixed random
    order (the same `seed` draws the same songs)."""
    found = []
    for path in sorted(Path(library).glob("*/song_chart.json")):
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        backing = doc.get("audio", {}).get("backing")
        kind = doc.get("meta", {}).get("source", {}).get("kind")
        if kind in NOT_WRITTEN or doc.get("arrangement", {}).get("type") == "bass" or not backing:
            continue
        if (path.parent / backing).is_file():
            found.append(path.parent)
    random.Random(seed).shuffle(found)
    return found[:limit] if limit else found


def _shrink(work: Path) -> None:
    """Keeps what measuring again needs: the guitar stem as mono FLAC instead of two float WAVs."""
    chart = Chart.load(work / "song_chart.json")
    wav = work / (chart.audio_guitar or "isolated_guitar.wav")
    if wav.suffix == ".wav" and wav.exists():
        import librosa

        y, _ = librosa.load(str(wav), sr=STEM_RATE, mono=True)
        sf.write(str(work / STEM), y, STEM_RATE, subtype="PCM_16")
        wav.unlink()
        chart.audio_guitar = STEM
    if chart.audio_backing:
        (work / chart.audio_backing).unlink(missing_ok=True)
        chart.audio_backing = None
    (work / "cover.jpg").unlink(missing_ok=True)
    chart.save(work / "song_chart.json")


def measure(
    song: Path, work: Path, *, mix: bool = False, again: bool = False, rechart: bool = False, ablation: bool = False, device: str | None = None
) -> SongResult:
    """One reference song through the pipeline (or its cached work), measured stage by stage."""
    from . import pipeline, transcription, tunings
    from .stem import GuitarStem

    ref = Chart.load(song / "song_chart.json")
    result = SongResult(song.name, ref.title, ref.artist, ref.arrangement_name, ref.tuning.name)
    cached = work / RESULT
    if cached.exists() and not (again or rechart or ablation):
        return SongResult.from_json(json.loads(cached.read_text(encoding="utf-8")))
    t0 = time.perf_counter()
    audio = song / ref.audio_backing
    if not (work / "song_chart.json").exists():
        pipeline.ingest_audio(audio, work, title=ref.title, artist=ref.artist, device=device)
        _shrink(work)
    elif rechart:
        pipeline.rechart(work, retune=True)
    est = Chart.load(work / "song_chart.json")
    result.tuning_found = est.tuning.name if est.tuning.strings == ref.tuning.strings else f"{est.tuning.name} (written: {ref.tuning.name})"
    reference = chart_notes(ref)
    raw_rows = json.loads((work / pipeline.TRANSCRIPTION_CACHE).read_text(encoding="utf-8"))
    raw = [transcription.RawNote(**n) for n in raw_rows]
    stem = GuitarStem.load(work / (est.audio_guitar or STEM))
    cleaned = transcription.clean(raw, tunings.ANALYSIS, stem=stem)
    stages = {"raw": raw_notes(raw_rows), "clean": Notes.of([(n.time, n.time + n.duration, float(n.pitch), None) for n in cleaned])}
    if mix:
        cache = work / MIX_CACHE
        if not cache.exists():
            cache.write_text(json.dumps([asdict(n) for n in transcription.basic_pitch(audio)]), encoding="utf-8")
        stages = {"mix": raw_notes(json.loads(cache.read_text(encoding="utf-8"))), **stages}
    stages["chart"] = chart_notes(est)
    for name, notes in stages.items():
        result.stages[name], matched = score(reference, notes)
        if name == "chart":
            result.fingering_matched = len(matched)
            result.fingering_same = sum(reference.places[i] == notes.places[j] for i, j in matched)
    if ablation:
        for step, off in CLEAN_STEPS.items():
            variant = transcription.clean(raw, tunings.ANALYSIS, stem=stem, **off)
            result.ablation[step] = score(reference, Notes.of([(n.time, n.time + n.duration, float(n.pitch), None) for n in variant]))[0]
    result.seconds = round(time.perf_counter() - t0, 1)
    cached.write_text(json.dumps(result.to_json(), ensure_ascii=False, indent=1), encoding="utf-8")
    return result


STAGES = ("mix", "raw", "clean", "chart")


def aggregate(results: list[SongResult]) -> dict:
    """Per stage: precision, recall and F1 over every note (songs add up), the median of the
    songs' F1, F1 at any octave and on timing alone, the median onset lag; then fingering and
    tuning."""
    done = [r for r in results if r.error is None and r.stages]
    out: dict = {"songs": len(done), "failed": len(results) - len(done), "stages": {}}
    for stage in STAGES:
        rows = [r.stages[stage] for r in done if stage in r.stages]
        if not rows:
            continue
        total = Score()
        for s in rows:
            total.add(s)
        lags = [s.lag_ms for s in rows if s.lag_ms is not None]
        out["stages"][stage] = {
            **asdict(total),
            "precision": total.matched / total.found if total.found else 0.0,
            "recall": total.matched / total.reference if total.reference else 0.0,
            "f1": total.f1(),
            "f1_median": float(np.median([s.f1() for s in rows])),
            "any_octave": total.f1(total.matched_any_octave),
            "timing": total.f1(total.matched_onset),
            "lag_ms": float(np.median(lags)) if lags else None,
        }
    out["ablation"] = {}
    for step in CLEAN_STEPS:
        rows = [r.ablation[step] for r in done if step in r.ablation]
        if rows:
            total = Score()
            for s in rows:
                total.add(s)
            out["ablation"][step] = {
                "songs": len(rows),
                "precision": total.matched / total.found if total.found else 0.0,
                "recall": total.matched / total.reference if total.reference else 0.0,
                "f1": total.f1(),
            }
    matched = sum(r.fingering_matched for r in done)
    out["fingering"] = sum(r.fingering_same for r in done) / matched if matched else None
    out["tuning"] = [sum(1 for r in done if r.tuning_found and "(written" not in r.tuning_found), len(done)]
    return out


def summary(results: list[SongResult]) -> str:
    """`aggregate` as a table, and the songs where the chart is furthest from what was written."""
    agg = aggregate(results)
    lines = [f"{agg['songs']} songs measured" + (f", {agg['failed']} failed" if agg["failed"] else "")]
    lines.append(f"{'stage':<6} {'precision':>9} {'recall':>7} {'F1':>6} {'F1 median':>9} {'any octave':>10} {'timing only':>11} {'lag ms':>7}")
    for stage, s in agg["stages"].items():
        lines.append(
            f"{stage:<6} {s['precision']:>9.1%} {s['recall']:>7.1%} {s['f1']:>6.1%} {s['f1_median']:>9.1%}"
            f" {s['any_octave']:>10.1%} {s['timing']:>11.1%} {s['lag_ms'] or 0.0:>7.1f}"
        )
    f = agg["fingering"]
    lines.append(f"fingering: {f:.1%} of matched chart notes on the written string and fret" if f is not None else "fingering: no matched note")
    lines.append(f"tuning found: {agg['tuning'][0]}/{agg['tuning'][1]}")
    done = [r for r in results if r.error is None and "chart" in r.stages]
    worst = sorted(done, key=lambda r: r.stages["chart"].f1())[:5]
    lines.append("lowest chart F1: " + "; ".join(f"{r.artist} — {r.title} {r.stages['chart'].f1():.0%}" for r in worst))
    return "\n".join(lines)


def _commit() -> str | None:
    """The pipeline's source revision (with + when it has uncommitted changes), if in a checkout."""
    import subprocess

    here = Path(__file__).resolve().parent
    try:
        rev = subprocess.run(["git", "-C", str(here), "rev-parse", "--short", "HEAD"], capture_output=True, text=True, timeout=5, check=False).stdout.strip()
        dirty = subprocess.run(["git", "-C", str(here), "status", "--porcelain", "--", "."], capture_output=True, text=True, timeout=5, check=False).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None
    return (rev + ("+" if dirty else "")) or None


class Run:
    """One bench run, recorded for the dashboard: `progress.json` while it goes (which song,
    how many), then `runs/<id>.json` with every song's result and the totals, rewritten after
    each song so a run cut short still counts."""

    def __init__(self, work: Path, total: int, options: dict, label: str | None = None):
        self.work = Path(work)
        self.started = time.time()
        self.id = time.strftime("%Y%m%d-%H%M%S", time.localtime(self.started))
        self.commit = _commit()
        self.label = label or self.commit or self.id
        self.total, self.options = total, options
        self.results: list[SongResult] = []
        self.current: str | None = None
        (self.work / "runs").mkdir(parents=True, exist_ok=True)

    def _write(self, name: Path, doc: dict) -> None:
        tmp = name.with_suffix(".tmp")
        tmp.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
        tmp.replace(name)

    def progress(self, current: str | None, finished: bool = False) -> None:
        self.current = current
        self._write(
            self.work / "progress.json",
            {
                "run": self.id,
                "label": self.label,
                "started": self.started,
                "total": self.total,
                "done": len(self.results),
                "current": current,
                "finished": time.time() if finished else None,
                "seconds": [r.seconds for r in self.results],
            },
        )

    def add(self, result: SongResult) -> None:
        self.results.append(result)
        self.save()
        self.progress(None)

    def save(self, finished: bool = False) -> None:
        self._write(
            self.work / "runs" / f"{self.id}.json",
            {
                "id": self.id,
                "label": self.label,
                "commit": self.commit,
                "started": self.started,
                "finished": time.time() if finished else None,
                "options": self.options,
                "total": self.total,
                "summary": aggregate(self.results),
                "songs": [r.to_json() for r in self.results],
            },
        )
