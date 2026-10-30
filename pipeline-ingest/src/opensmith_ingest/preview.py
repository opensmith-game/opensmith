"""Plain-text tablature of a chart at one difficulty level, bar by bar (16th-note columns)."""

from __future__ import annotations

import bisect

from .chart import Chart

STRING_NAMES = "EADGBe"


def render(chart: Chart, level: int, bars_per_line: int = 4, start: float = 0.0, end: float | None = None) -> str:
    end = chart.duration if end is None else end
    bar_times = [b.time for b in chart.beats if b.bar]
    if not bar_times:
        return ""
    bar_times.append(bar_times[-1] + (bar_times[-1] - bar_times[-2] if len(bar_times) > 1 else 2.0))
    cols = 16
    n_strings = len(chart.tuning.strings)
    notes, chords = chart.events_at(level)
    cells: dict[tuple[int, int, int], str] = {}

    def put(t: float, string: int, fret: int) -> None:
        b = bisect.bisect_right(bar_times, t + 1e-6) - 1
        if b < 0 or b >= len(bar_times) - 1:
            return
        col = min(round((t - bar_times[b]) / (bar_times[b + 1] - bar_times[b]) * cols), cols - 1)
        cells[(b, col, string)] = str(fret)

    for n in notes:
        put(n.time, n.string, n.fret)
    for c in chords:
        for s, f in enumerate(chart.templates[c.template].frets):
            if f >= 0:
                put(c.time, s, f)

    first = max(bisect.bisect_right(bar_times, start) - 1, 0)
    last = min(bisect.bisect_left(bar_times, end), len(bar_times) - 1)
    lines = []
    for line_start in range(first, last, bars_per_line):
        bars = range(line_start, min(line_start + bars_per_line, last))
        lines.append("   " + "".join(f"{b + 1:<{cols * 3 + 1}}" for b in bars))
        for s in reversed(range(n_strings)):
            row = f"{STRING_NAMES[s] if s < len(STRING_NAMES) else s}|"
            for b in bars:
                row += "".join(cells.get((b, c, s), "-").rjust(2, "-") + "-" for c in range(cols)) + "|"
            lines.append(row)
        lines.append("")
    return "\n".join(lines)
