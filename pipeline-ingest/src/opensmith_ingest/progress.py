"""Import progress: stages weighted by their usual share of the time, reported as one fraction.

The app shows a single bar for an import; stem separation alone takes about half of it, so
equal steps would make the bar lie. Weights come from a 4-minute song on an M1 Max (GPU).
"""

from __future__ import annotations

from collections.abc import Callable

STAGES: dict[str, float] = {
    "decode": 0.03,
    "separate": 0.52,
    "stems": 0.04,
    "transcribe": 0.2,
    "beats": 0.12,
    "chart": 0.09,
}

# (stage, fraction of the whole import done)
Sink = Callable[[str, float], None]


class Progress:
    def __init__(self, sink: Sink | None = None, stages: dict[str, float] = STAGES):
        self._sink = sink
        self._stages = stages
        self._total = sum(stages.values())
        self._before = 0.0
        self._stage: str | None = None
        self._last = 0.0

    def stage(self, name: str) -> None:
        """Starts stage `name` (the previous one is complete)."""
        if self._stage is not None:
            self._before += self._stages[self._stage]
        self._stage = name
        self.update(0.0)

    def update(self, within: float) -> None:
        """Fraction of the current stage done. The overall fraction never goes back."""
        if self._stage is None:
            return
        f = (self._before + self._stages[self._stage] * min(max(within, 0.0), 1.0)) / self._total
        self._last = max(self._last, min(f, 1.0))
        if self._sink:
            self._sink(self._stage, self._last)

    def done(self) -> None:
        self._last = 1.0
        if self._sink:
            self._sink("done", 1.0)
