"""Automatic tuning detection and tempo choice on syncopated rock."""

import numpy as np
import pytest

from opensmith_ingest import beats
from opensmith_ingest.fingering import Note
from opensmith_ingest.synth import _hat, _kick, _snare, rock_song
from opensmith_ingest.tunings import detect, name_of


def power_chord_riff(roots: list[int], repeats: int = 8, step: float = 0.375) -> list[Note]:
    """Root-fifth-octave power chords, then a single-note line on the low string area."""
    notes, t = [], 0.0
    for _ in range(repeats):
        for r in roots:
            notes += [Note(t, r, step), Note(t, r + 7, step), Note(t, r + 12, step)]
            t += step
            notes.append(Note(t, r, step / 2))
            t += step / 2
            notes.append(Note(t, r + 3, step / 2))
            t += step / 2
    return notes


def test_standard_tuning_is_kept_for_standard_material():
    choice = detect(rock_song().guitar_notes)
    assert choice.tuning.name == "E Standard" and choice.below_standard == 0


def test_drop_d_riff_is_detected():
    choice = detect(power_chord_riff([38, 41, 43, 38]))  # D5 F5 G5 D5 from the low D
    assert (choice.tuning.name, choice.lowest_played) == ("Drop D", 38)


def test_drop_c_riff_is_detected():
    choice = detect(power_chord_riff([36, 39, 41, 44]))  # C5 Eb5 F5 G#5
    assert (choice.tuning.name, choice.lowest_played) == ("Drop C", 36)


def test_a_few_stray_low_notes_do_not_retune_the_guitar():
    notes = rock_song().guitar_notes + [Note(3.0, 38, 0.1)]  # one bass-bleed D2 among ~115 notes
    assert len(notes) > 100
    choice = detect(notes)
    assert choice.tuning.name == "E Standard" and choice.below_standard == 1


def syncopated_rock(bpm: float = 80.0, bars: int = 16, sr: int = 22_050) -> np.ndarray:
    """4/4 rock beat with a loud 3+3+2 eighth-note accent riff on top."""
    rng = np.random.default_rng(1)
    beat = 60.0 / bpm
    n = int((bars * 4 * beat + 1.0) * sr)
    y = np.zeros(n)

    def add(sig, t, gain):
        i = int(t * sr)
        j = min(i + sig.size, n)
        y[i:j] += gain * sig[: j - i]

    t = np.arange(int(0.3 * sr)) / sr
    accent = (np.sin(2 * np.pi * 65.4 * t) + 0.6 * rng.standard_normal(t.size)) * np.exp(-t / 0.08)
    for bar in range(bars):
        t0 = 0.5 + bar * 4 * beat
        for b in range(4):
            add(_kick(sr) if b % 2 == 0 else _snare(sr, rng), t0 + b * beat, 0.5)
        for e in range(8):
            add(_hat(sr, rng), t0 + e * beat / 2, 0.6)
        for e in (0, 3, 6):  # 3 + 3 + 2
            add(accent, t0 + e * beat / 2, 1.2)
    return y


@pytest.mark.parametrize("bpm", [80.0, 95.0])
def test_syncopated_riff_does_not_fool_the_tempo(bpm):
    import librosa

    sr = 22_050
    y = syncopated_rock(bpm)
    onset = librosa.onset.onset_strength(y=y, sr=sr, hop_length=beats.HOP)
    tempo = beats.choose_tempo(onset, sr)
    ratio = tempo / bpm
    assert min(abs(ratio - 1), abs(ratio - 2), abs(ratio - 0.5)) < 0.02, f"{tempo} BPM for a {bpm} BPM song"
    grid = beats.track(y, sr, len(y) / sr, rhythm=y)
    assert abs(grid.bpm / bpm - round(grid.bpm / bpm)) < 0.02 and round(grid.bpm / bpm) in (1, 2)


def test_a_standard_tuning_moved_down_is_told_from_a_drop_tuning():
    # A riff a whole step down, single notes: D D# E G E D A G E D# D G D D.
    riff = [38, 39, 40, 43, 40, 38, 45, 43, 40, 39, 38, 43, 38, 38] * 8
    choice = detect([Note(0.25 * k, p, 0.25) for k, p in enumerate(riff)])
    assert choice.tuning.name == "D Standard"
    # A drop-tuning riff without chords leans on the fifth, its open second string.
    pedal = [36, 36, 43, 36, 36, 43, 44, 43] * 8
    assert detect([Note(0.25 * k, p, 0.25) for k, p in enumerate(pedal)]).tuning.name == "Drop C"


def test_bass_bleeding_into_the_stem_does_not_lower_the_tuning():
    notes = [Note(0.3 * k, p, 0.3) for k, p in enumerate([40, 55, 59, 64, 59, 55] * 40)]
    notes += [Note(7.3 * k, 38, 0.3) for k in range(3)]  # a few D2 from the bass
    assert detect(notes).tuning.name == "E Standard"


def test_tunings_are_named_like_guitarists_do():
    e, bass = (40, 45, 50, 55, 59, 64), (28, 33, 38, 43)
    assert name_of(e, e) == "E Standard"
    assert name_of((39, 44, 49, 54, 58, 63), e) == "Eb Standard"
    assert name_of((38, 45, 50, 55, 59, 64), e) == "Drop D"
    assert name_of((37, 44, 49, 54, 58, 63), e) == "Drop C#"
    assert name_of((36, 43, 48, 53, 57, 62), e) == "Drop C"
    assert name_of((38, 43, 50, 55, 59, 62), e) == "Open G"
    assert name_of((38, 45, 50, 55, 57, 62), e) == "DADGAD"
    assert name_of((26, 33, 38, 43), bass) == "Drop D"
    assert name_of((27, 32, 37, 42), bass) == "Eb Standard"
    assert name_of((40, 45, 50, 55, 60, 64), e) == "E A D G C E"
