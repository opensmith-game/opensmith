"""The bench's dashboard: a page on 127.0.0.1 reading the bench's work folder as it fills.

GET /             the page (bench_view.html)
GET /api/state    the run under way (progress.json), every recorded run's totals, the latest
                  run and the one before in full (song by song gains; without recorded runs,
                  the songs measured so far), and how much of the library has been analysed
"""

from __future__ import annotations

import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from .bench import SongResult, aggregate, references

PAGE = Path(__file__).with_name("bench_view.html")
# Counting the library's reference songs reads every chart: once a minute is enough.
_library: dict[str, tuple[float, int]] = {}


def _references(library: Path) -> int:
    key = str(library)
    at, count = _library.get(key, (0.0, 0))
    if time.time() - at > 60:
        count = len(references(library))
        _library[key] = (time.time(), count)
    return count


def _read(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def _songs(run: dict) -> tuple:
    """Which songs a run measured: its draw (seed, offset, limit)."""
    o = run.get("options") or {}
    return (o.get("seed"), o.get("offset", 0), o.get("limit"))


def state(work: Path, library: Path | None = None) -> dict:
    """What the page shows, read from `work` on every call."""
    work = Path(work)
    measured = [s for s in (_read(p) for p in work.glob("*/bench.json")) if s]
    # A song's first measure includes separating it; later ones only re-measure.
    first = sorted(s["seconds"] for s in measured if s.get("seconds", 0) > 15)
    coverage = {
        "analysed": len(measured),
        "total": _references(library) if library is not None and Path(library).is_dir() else None,
        "seconds_per_song": first[len(first) // 2] if first else None,
    }
    runs = [r for r in (_read(p) for p in sorted((work / "runs").glob("*.json"))) if r]
    # The latest finished run (a run just started has a song or two), compared with the runs
    # made on the same songs only: the one before it and the first.
    finished = [r for r in runs if r.get("finished")]
    latest = finished[-1] if finished else (runs[-1] if runs else None)
    same = [r for r in finished if latest is not None and r["id"] != latest["id"] and _songs(r) == _songs(latest)]
    if latest is None:
        # Songs measured before runs were recorded: one run made of them.
        songs = [s for s in (_read(p) for p in sorted(work.glob("*/bench.json"))) if s]
        if songs:
            results = [SongResult.from_json(dict(s)) for s in songs]
            latest = {"id": "cache", "label": "cache", "started": None, "finished": None, "options": {}, "total": len(songs), "summary": aggregate(results), "songs": songs}
    return {
        "progress": _read(work / "progress.json"),
        "runs": [{k: r.get(k) for k in ("id", "label", "commit", "started", "finished", "options", "total", "summary")} for r in runs],
        "latest": latest,
        "previous": same[-1] if same else None,
        "baseline": {k: same[0].get(k) for k in ("id", "label", "summary")} if same else None,
        "coverage": coverage,
    }


class _Handler(BaseHTTPRequestHandler):
    work: Path
    library: Path | None

    def do_GET(self) -> None:  # the name http.server calls
        if self.path in ("/", "/index.html"):
            body, kind = PAGE.read_bytes(), "text/html; charset=utf-8"
        elif self.path == "/api/state":
            body, kind = json.dumps(state(self.work, self.library), ensure_ascii=False).encode("utf-8"), "application/json"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args) -> None:  # the page polls: no line per request
        pass


def server(work: Path, port: int = 8765, library: Path | None = None) -> ThreadingHTTPServer:
    handler = type("Handler", (_Handler,), {"work": Path(work), "library": Path(library) if library else None})
    return ThreadingHTTPServer(("127.0.0.1", port), handler)


def serve(work: Path, port: int = 8765, library: Path | None = None) -> None:
    httpd = server(work, port, library)
    print(f"bench dashboard: http://127.0.0.1:{httpd.server_address[1]}/ (reading {work})", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
