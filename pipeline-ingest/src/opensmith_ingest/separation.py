"""Stem separation with Demucs `htdemucs_6s` (drums, bass, other, vocals, guitar, piano)."""

from __future__ import annotations

import math
import os
import time
from collections.abc import Callable
from dataclasses import dataclass

import numpy as np

MODEL = "htdemucs_6s"


@dataclass
class Stems:
    rate: int
    mix: np.ndarray
    sources: dict[str, np.ndarray]
    device: str
    seconds: float

    stem: str = "guitar"

    def energy_db(self, name: str) -> float:
        x = self.sources[name]
        return float(10 * np.log10(np.mean(x * x) + 1e-12))

    def choose_guitar(self, stem: str = "auto", margin_db: float = 20.0) -> str:
        """Which stems carry the guitar part.

        `htdemucs_6s` sometimes files a guitar under "other" (clean or unusual tones). In
        "auto" mode the guitar stem is used alone unless it is more than `margin_db` quieter
        than "other", in which case both are used.
        """
        if stem == "auto":
            stem = "guitar" if self.energy_db("guitar") > self.energy_db("other") - margin_db else "guitar+other"
        self.stem = stem
        return stem

    @property
    def guitar(self) -> np.ndarray:
        return sum(self.sources[name] for name in self.stem.split("+"))

    @property
    def backing(self) -> np.ndarray:
        """Everything but the guitar: the exact complement of the guitar part in the mix."""
        return self.mix - self.guitar


def best_device() -> str:
    import torch

    return "mps" if torch.backends.mps.is_available() else "cpu"


def _evaluations(model, length: int, shifts: int, overlap: float = 0.25) -> int:
    """How many segments `apply_model` will run the network on (progress denominator)."""
    count = 0
    for m in getattr(model, "models", [model]):
        rate = m.samplerate
        segment = int(rate * float(getattr(m, "segment", 7.8)))
        stride = max(1, int((1 - overlap) * segment))
        padded = length + (int(0.5 * rate) if shifts else 0)
        count += math.ceil(padded / stride) * max(1, shifts)
    return max(count, 1)


def separate(
    mix: np.ndarray, device: str | None = None, shifts: int = 1, progress: Callable[[float], None] | None = None
) -> Stems:
    """Separates a `(2, samples)` mix sampled at 44.1 kHz (the model's rate); `progress`
    receives the fraction of the work done."""
    os.environ.setdefault("PYTORCH_ENABLE_MPS_FALLBACK", "1")
    import torch
    from demucs.apply import apply_model
    from demucs.pretrained import get_model

    device = device or best_device()
    model = get_model(MODEL)
    model.eval()
    wav = torch.from_numpy(np.ascontiguousarray(mix, dtype=np.float32))
    # Demucs expects roughly unit-variance input.
    ref = wav.mean(0)
    mean, std = ref.mean(), ref.std() + 1e-8
    total = _evaluations(model, wav.shape[-1], shifts)
    finished = 0

    def segment_done(state: dict) -> None:
        nonlocal finished
        if state.get("state") == "end" and progress is not None:
            finished += 1
            progress(min(finished / total, 1.0))

    t0 = time.perf_counter()
    with torch.no_grad():
        out = apply_model(
            model,
            ((wav - mean) / std)[None],
            device=device,
            shifts=shifts,
            split=True,
            overlap=0.25,
            progress=False,
            callback=segment_done,
        )[0]
    out = out * std + mean / len(model.sources)
    sources = {name: out[k].cpu().numpy().astype(np.float32) for k, name in enumerate(model.sources)}
    return Stems(model.samplerate, mix.astype(np.float32), sources, device, time.perf_counter() - t0)
