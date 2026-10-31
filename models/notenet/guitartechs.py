"""Guitar-TECHS (CC BY 4.0, https://guitar-techs.github.io) items into OpenSmith takes: the direct
input as the cable would carry it (gain set for -12 dBFS peaks, as the app's input calibration
does), the per-string MIDI as a chart (notes, chords, techniques), each onset moved onto the real
attack (the MIDI pickup reports ~19 ms late) and the pickup's ghost notes left out.

uv run --project pipeline-ingest python -I models/notenet/guitartechs.py <unzipped items> <takes out>
"""

import json
import sys
from pathlib import Path

import mido
import numpy as np
import soundfile as sf

SR = 48_000
OPEN = {"E": 40, "A": 45, "D": 50, "G": 55, "B": 59, "e": 64}
STRING = {"E": 0, "A": 1, "D": 2, "G": 3, "B": 4, "e": 5}
CHORD_SPREAD = 0.05  # onsets this close form one strum
HARMONIC_NODE = {12: 12, 19: 7, 24: 5, 28: 4}  # sounding interval above the open string → node fret


def notes_of(midi_path: Path):
    """(onset s, offset s, string index, midi pitch, bend semitones) of every note."""
    m = mido.MidiFile(midi_path)
    tempo = 500_000
    for msg in m.tracks[0]:
        if msg.type == "set_tempo":
            tempo = msg.tempo
    out = []
    for track in m.tracks[1:]:
        name = track.name.strip()
        if name not in STRING:
            continue
        now, on, bend, peak = 0, {}, 0.0, {}
        for msg in track:
            now += msg.time
            t = mido.tick2second(now, m.ticks_per_beat, tempo)
            if msg.type == "pitchwheel":
                bend = msg.pitch / 8192 * 2.0  # ±2 semitones range
                for k in peak:
                    peak[k] = max(peak[k], bend)
            elif msg.type == "note_on" and msg.velocity > 0:
                on[msg.note] = t
                peak[msg.note] = 0.0
            elif msg.type in ("note_off", "note_on") and msg.note in on:
                start = on.pop(msg.note)
                out.append((start, t, STRING[name], msg.note, peak.pop(msg.note, 0.0)))
    return sorted(out)


def clean(notes):
    """The MIDI pickup's ghosts out: notes shorter than 50 ms, and of notes overlapping on one
    string (a string sounds one note at a time), all but the longest."""
    notes = [n for n in notes if n[1] - n[0] >= 0.05]
    keep = []
    for n in sorted(notes, key=lambda n: n[1] - n[0], reverse=True):
        if not any(k[2] == n[2] and k[0] < n[1] - 0.05 and n[0] < k[1] - 0.05 for k in keep):
            keep.append(n)
    return sorted(keep)


def attack_near(env: np.ndarray, t: float) -> float:
    """The real attack near MIDI onset `t`: the first millisecond, from 60 ms before to 20 ms
    after, louder than 10× the median of the 50 ms before that span; else 19 ms earlier."""
    i = int(t * 1000)
    lo, hi = max(i - 60, 60), min(i + 20, len(env) - 1)
    if hi <= lo:
        return t - 0.019
    floor = np.median(env[lo - 50 : lo]) + 1e-9
    above = np.where(env[lo:hi] > 10 * floor)[0]
    return (lo + above[0]) / 1000 if len(above) else t - 0.019


def chart(title: str, notes, duration: float, technique: str | None) -> dict:
    events, k = [], 0
    while k < len(notes):
        group = [notes[k]]
        while k + len(group) < len(notes) and notes[k + len(group)][0] - notes[k][0] <= CHORD_SPREAD:
            group.append(notes[k + len(group)])
        events.append(group)
        k += len(group)
    tech = {"palm_mute": {"palmMute": True}, "vibrato": {"vibrato": True}, "pinch": {"pinchHarmonic": True}}.get(technique or "")
    out_notes, out_chords, templates = [], [], []
    for group in events:
        t = group[0][0]
        if len({g[2] for g in group}) < len(group) or len(group) == 1:
            for start, end, s, pitch, bend in group:
                fret = pitch - [40, 45, 50, 55, 59, 64][s]
                techniques = dict(tech or {})
                if technique == "harmonic":
                    node = HARMONIC_NODE.get(fret)
                    if node is None:
                        continue
                    fret, techniques = node, {"harmonic": True}
                if technique == "bend" and bend >= 0.4:
                    techniques["bend"] = round(bend * 2) / 2
                if not 0 <= fret <= 24:
                    continue
                n = {"time": round(start, 4), "string": s, "fret": fret, "sustain": round(max(end - start, 0.0), 3), "levels": [0, 0]}
                if techniques:
                    n["techniques"] = techniques
                out_notes.append(n)
            continue
        frets = [-1] * 6
        for _, _, s, pitch, _ in group:
            frets[s] = pitch - [40, 45, 50, 55, 59, 64][s]
        if any(f > 24 for f in frets):
            continue
        if frets not in [tp["frets"] for tp in templates]:
            templates.append({"name": "", "frets": frets, "fingers": [-1] * 6})
        c = {"time": round(t, 4), "template": [tp["frets"] for tp in templates].index(frets), "sustain": round(max(g[1] for g in group) - t, 3), "levels": [0, 0]}
        if tech:
            c["techniques"] = tech
        out_chords.append(c)
    return {
        "format": "opensmith-chart",
        "version": 1,
        "meta": {"title": title, "artist": "Guitar-TECHS", "album": None, "year": None, "duration": duration, "source": {"kind": "guitartechs", "file": title}, "generator": "guitartechs.py"},
        "audio": {"backing": None, "guitar": None, "mix": None, "offset": 0.0},
        "arrangement": {"name": "Lead", "type": "lead", "tuning": {"name": "E Standard", "strings": [40, 45, 50, 55, 59, 64]}, "capo": 0, "maxDifficulty": 0},
        "tempo": {"bpm": 60.0, "timeSignature": [4, 4]},
        "beats": [{"time": float(b), "bar": (b // 4) + 1 if b % 4 == 0 else None} for b in range(int(duration) + 1)],
        "sections": [],
        "phrases": [{"name": "all", "start": 0.0, "end": duration, "maxDifficulty": 0}],
        "chordTemplates": templates,
        "notes": out_notes,
        "chords": out_chords,
        "anchors": [],
        "handShapes": [],
    }


def technique_of(name: str) -> str | None:
    n = name.lower()
    for key, tech in [("palm", "palm_mute"), ("vibrato", "vibrato"), ("pinch", "pinch"), ("harmonic", "harmonic"), ("bend", "bend")]:
        if key in n:
            return tech
    return None


def main(data: Path, takes: Path):
    for di in sorted(data.glob("*/audio/directinput/*.wav")):
        item = di.parent.parent.parent
        stem = di.stem.replace("directinput_", "")
        midi = item / "midi" / f"midi_{stem}.mid"
        if not midi.exists():
            print("no MIDI for", di)
            continue
        x, sr = sf.read(di, dtype="float32", always_2d=True)
        x = x.mean(axis=1)
        if sr != SR:
            from math import gcd

            from scipy.signal import resample_poly

            g = gcd(SR, sr)
            x = resample_poly(x, SR // g, sr // g).astype(np.float32)
        gain = 10 ** (-12 / 20) / max(np.percentile(np.abs(x), 99.99), 1e-6)
        x = np.clip(x * gain, -1, 1).astype(np.float32)
        env = np.abs(x[: len(x) // 48 * 48]).reshape(-1, 48).mean(axis=1)
        notes = [(attack_near(env, on), off, s, p, b) for on, off, s, p, b in notes_of(midi)]
        notes = clean([n for n in notes if n[0] > 0.05])
        duration = len(x) / SR
        name = f"{item.name}_{stem}"
        doc = chart(name, notes, duration, technique_of(item.name + stem))
        out = takes / name
        out.mkdir(parents=True, exist_ok=True)
        sf.write(out / "guitar.wav", x, SR, subtype="FLOAT")
        (out / "chart.json").write_text(json.dumps(doc))
        meta = {"version": 1, "title": name, "sampleRate": SR, "anchors": [[0, 0.0]], "inputOffset": 0.0, "levels": [0], "score": {"hits": 0, "perfects": 0, "misses": 0, "extras": 0}}
        (out / "take.json").write_text(json.dumps(meta, indent=1))
        print(f"{name}: {len(doc['notes'])} notes, {len(doc['chords'])} chords, {duration:.0f} s, gain {20 * np.log10(gain):+.1f} dB")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
