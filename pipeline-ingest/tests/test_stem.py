"""Analyses of the guitar stem: YIN pitch with weak fundamentals, attacks section by section,
silence."""

import librosa
import numpy as np

from opensmith_ingest.stem import RATE, GuitarStem
from opensmith_ingest.synth import _amp, _string, midi_hz


def tone(midi: int, dur: float, partials: dict[int, float], t60: float = 1.5) -> np.ndarray:
    """Decaying additive note: harmonic number → amplitude."""
    t = np.arange(int(dur * RATE)) / RATE
    f = midi_hz(midi)
    y = sum(a * np.sin(2 * np.pi * f * h * t) for h, a in partials.items() if f * h < RATE / 2)
    return y * np.exp(-6.9 * t / t60) * np.minimum(t / 0.003, 1.0)


# A low note through an amp and a mix: the fundamental 30 dB under the octave, odd harmonics weak.
WEAK_FUNDAMENTAL = {1: 0.03, 2: 1.0, 3: 0.25, 4: 0.5, 5: 0.1, 6: 0.2}
PLAIN = {1: 1.0, 2: 0.5, 3: 0.3, 4: 0.2}


def test_yin_hears_a_low_note_whose_fundamental_is_weak():
    stem = GuitarStem(np.concatenate([tone(40, 1.0, WEAK_FUNDAMENTAL), tone(52, 1.0, PLAIN)]))
    midi, aperiodicity = stem.pitch([0.1, 0.3, 1.1, 1.3])
    assert np.allclose(midi[:2], 40, atol=0.3), midi  # E2, not the E3 its spectrum peaks at
    assert np.allclose(midi[2:], 52, atol=0.3), midi  # a real E3 stays E3
    assert (aperiodicity < 0.2).all()
    # The spectrum alone points an octave up: what Basic Pitch hears.
    spectrum = np.abs(np.fft.rfft(stem.y[: RATE // 2]))
    assert abs(np.fft.rfftfreq(RATE // 2, 1 / RATE)[spectrum.argmax()] - midi_hz(52)) < 3


def add(buf: np.ndarray, sig: np.ndarray, t: float) -> None:
    i = int(t * RATE)
    j = min(i + sig.size, buf.size)
    buf[i:j] += sig[: j - i]


def test_attacks_are_found_section_by_section():
    """A clean, sparse intro (big attacks out of silence), then a dense distorted part (attacks
    a few dB over a wall of sustain): the case where one threshold for the whole song fails."""
    rng = np.random.default_rng(0)
    intro, heavy = np.zeros(8 * RATE), np.zeros(int(8.5 * RATE))
    clean = [0.25 + 0.5 * k for k in range(15)]
    dense = [0.25 + 0.25 * k for k in range(32)]
    for t in clean:
        add(intro, 0.5 * _string(midi_hz(int(rng.integers(45, 64))), 0.45, RATE, rng, t60=0.3), t)
    for t in dense:
        add(heavy, 0.3 * _string(midi_hz(int(rng.choice([40, 43, 45]))), 0.6, RATE, rng, t60=4.0), t)
    heavy = _amp(heavy, RATE, gain=30.0)
    heavy *= 0.5 / np.abs(heavy).max()
    y = np.concatenate([intro, heavy]) + 1e-4 * rng.standard_normal(intro.size + heavy.size)
    truth = np.array(clean + [8 + t for t in dense])
    stem = GuitarStem(y)

    def recall(times: np.ndarray, part: np.ndarray) -> float:
        return float(np.mean([np.min(np.abs(times - t)) < 0.03 for t in part])) if len(times) else 0.0

    found = stem.attacks
    assert recall(found, truth[truth < 8]) > 0.9
    assert recall(found, truth[truth >= 8]) > 0.9
    assert np.mean([np.min(np.abs(truth - t)) < 0.03 for t in found]) > 0.9, "few false attacks"
    whole = librosa.onset.onset_detect(y=stem.y, sr=RATE, hop_length=128, units="time")
    assert recall(whole, truth[truth >= 8]) < 0.5, "one threshold for the song misses the heavy part"


def test_silence_is_not_guitar():
    y = np.concatenate([np.zeros(2 * RATE), tone(45, 2.0, PLAIN), 1e-5 * np.ones(RATE)])
    stem = GuitarStem(y)
    assert not stem.audible(1.0)
    assert stem.audible(2.5)
    assert not stem.audible(4.5), "60 dB down is residue, not playing"
