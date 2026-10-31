"""Fingering benchmark against GuitarSet (Xi et al., ISMIR 2018, CC BY 4.0).

GuitarSet records 360 excerpts with a hexaphonic pickup, so every annotated note carries the
string the guitarist actually used: human tablature to compare the solver with.
"""

from __future__ import annotations

import json
import zipfile
from dataclasses import dataclass, field
from pathlib import Path
from urllib.request import urlretrieve

from .fingering import FingeringSolver, Note, Tuning

ANNOTATIONS_URL = "https://zenodo.org/api/records/3371780/files/annotation.zip/content"
DEFAULT_CACHE = Path.home() / ".cache" / "opensmith" / "guitarset"


def fetch_annotations(cache: Path = DEFAULT_CACHE) -> Path:
    """Downloads and unpacks the 39 MB annotation archive once; returns the JAMS directory."""
    target = cache / "annotation"
    if target.is_dir() and any(target.glob("*.jams")):
        return target
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / "annotation.zip"
    if not archive.exists():
        urlretrieve(ANNOTATIONS_URL, archive)
    with zipfile.ZipFile(archive) as z:
        z.extractall(target)
    return target


@dataclass(frozen=True)
class TrueNote:
    time: float
    pitch: int
    string: int
    duration: float


def load_jams(path: Path, tuning: Tuning = Tuning()) -> list[TrueNote]:
    doc = json.loads(path.read_text(encoding="utf-8"))
    notes = []
    for ann in doc["annotations"]:
        if ann["namespace"] != "note_midi":
            continue
        string = int(ann["annotation_metadata"]["data_source"])
        for obs in ann["data"]:
            pitch = round(obs["value"])
            if pitch - tuning.strings[string] < 0:
                continue
            notes.append(TrueNote(obs["time"], pitch, string, obs["duration"]))
    return sorted(notes, key=lambda n: (n.time, n.pitch))


@dataclass
class Score:
    notes: int = 0
    same_string: int = 0
    baseline_same_string: int = 0
    by_style: dict = field(default_factory=dict)

    @property
    def accuracy(self) -> float:
        return self.same_string / max(self.notes, 1)

    @property
    def baseline(self) -> float:
        return self.baseline_same_string / max(self.notes, 1)


def lowest_fret(pitch: int, tuning: Tuning) -> int:
    """Baseline: the position closest to the nut."""
    return min(tuning.positions(pitch), key=lambda p: p[1])[0]


def evaluate(paths: list[Path], tuning: Tuning = Tuning()) -> Score:
    score = Score()
    solver = FingeringSolver(tuning)
    for path in paths:
        truth = load_jams(path, tuning)
        if not truth:
            continue
        notes = [Note(n.time, n.pitch, n.duration) for n in truth]
        tab = solver.solve(notes, strict=False)
        played: dict[tuple[float, int], int] = {}
        for sl in tab:
            for note, (string, _) in zip(sl.notes, sl.placements):
                played[(note.time, note.pitch)] = string
        style = "comp" if "_comp" in path.name else "solo"
        hits = base = 0
        for n in truth:
            hits += played.get((n.time, n.pitch)) == n.string
            base += lowest_fret(n.pitch, tuning) == n.string
        score.notes += len(truth)
        score.same_string += hits
        score.baseline_same_string += base
        s = score.by_style.setdefault(style, [0, 0])
        s[0] += len(truth)
        s[1] += hits
    return score
