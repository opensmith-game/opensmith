"""The bench's dashboard: a page on 127.0.0.1 reading the bench's work folder as it fills.

GET /             the page (bench_view.html)
GET /api/state    the run under way (progress.json), every recorded run's totals, the latest
                  run and the one before in full (song by song gains; without recorded runs,
                  the songs measured so far), how much of the library has been analysed, and
                  the trainings described in the --trainings folder as their logs stand

A training description is a JSON file:

    {"title": "...", "about": "what it does, in a sentence", "built": ["how it is made", ...],
     "log": "train.log", "steps": 20000,
     "scores": [{"match": "val F1", "label": "..."}, ...],      the first one is the main score
     "compare": {"label": "...", "log": "older.log"},           optional: the same score, older
     "targets": [{"label": "...", "value": 41.8}],               optional: lines to beat
     "final": {"label": "...", "log": "eval.log", "pattern": "test .*?F1 ([\\d.]+)"},  optional
     "note": "...", "order": 1}                                   optional

Paths are relative to the description (or start with ~). The log is read for "step N" lines
(with "(x s/step)" or an elapsed "T s" at the end, for the pace), each score as "<match> <value>"
dated at the last step seen, and a line starting with "best " when the training is over.
"""

from __future__ import annotations

import json
import re
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


# A training whose log has not grown for this long has stopped (a check can take minutes).
_QUIET = 15 * 60
_STEP = re.compile(r"\bstep (\d+)\b")
_RATE = re.compile(r"\(([\d.]+) s/step\)")
_ELAPSED = re.compile(r"\b(\d+(?:\.\d+)?) s$")


def follow(log: Path, scores: list[str], done: str = r"^best ") -> dict | None:
    """A training's log as it stands: the step it is at, its pace (seconds a step), each score
    printed along (`<name> <value>`, dated at the last step seen) and whether it is over."""
    try:
        lines = Path(log).read_text(encoding="utf-8", errors="replace").splitlines()
        updated = Path(log).stat().st_mtime
    except OSError:
        return None
    named = {name: re.compile(re.escape(name) + r"\s+(-?\d+(?:\.\d+)?)") for name in scores}
    end = re.compile(done)
    step, rate, clock, finished = 0, None, None, False
    curves: dict[str, dict[int, float]] = {name: {} for name in scores}
    for line in lines:
        if end.search(line):  # the summary repeats the best score: not a new point
            finished = True
            continue
        if m := _STEP.search(line):
            step = int(m.group(1))
            if r := _RATE.search(line):
                rate = float(r.group(1))
            elif e := _ELAPSED.search(line):
                at = float(e.group(1))
                if clock and step > clock[0]:
                    rate = (at - clock[1]) / (step - clock[0])
                clock = (step, at)
        for name, pattern in named.items():
            if v := pattern.search(line):
                curves[name][step] = float(v.group(1))
    return {"step": step, "rate": rate, "finished": finished, "updated": updated, "curves": {name: sorted(c.items()) for name, c in curves.items()}}


def _beside(desc: Path, path: str) -> Path:
    return desc.parent / Path(path).expanduser()


def _final(desc: Path, final: dict) -> float | None:
    try:
        found = re.search(final["pattern"], _beside(desc, final["log"]).read_text(encoding="utf-8", errors="replace"), re.M)
    except (OSError, KeyError, re.error):
        return None
    return float(found.group(1)) if found else None


def trainings(folder: Path, now: float | None = None) -> list[dict]:
    """The trainings described in `folder` (see the module's doc), each with its log as it stands."""
    now = time.time() if now is None else now
    out = []
    for desc in sorted(Path(folder).glob("*.json")):
        d = _read(desc)
        if not d or not d.get("log") or not d.get("scores"):
            continue
        names = [s["match"] for s in d["scores"]]
        log = follow(_beside(desc, d["log"]), names, d.get("done", r"^best "))
        if log is None:
            status = "missing"
        elif log["finished"]:
            status = "finished"
        else:
            status = "running" if now - log["updated"] < _QUIET else "stopped"
        compare = d.get("compare")
        if compare:
            older = follow(_beside(desc, compare["log"]), names[:1])
            compare = {"label": compare.get("label"), "curve": older["curves"][names[0]] if older else []}
        final = d.get("final")
        out.append({
            "id": desc.stem,
            "order": d.get("order", 99),
            **{k: d.get(k) for k in ("title", "about", "built", "note", "steps", "targets")},
            "status": status,
            "step": log["step"] if log else 0,
            "rate": log["rate"] if log else None,
            "updated": log["updated"] if log else None,
            "scores": [{"label": s.get("label", s["match"]), "curve": log["curves"][s["match"]] if log else []} for s in d["scores"]],
            "compare": compare,
            "final": {"label": final.get("label"), "value": _final(desc, final)} if final else None,
        })
    return sorted(out, key=lambda t: t["order"])


def state(work: Path, library: Path | None = None, folder: Path | None = None) -> dict:
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
        "trainings": trainings(folder) if folder is not None and Path(folder).is_dir() else [],
    }


class _Handler(BaseHTTPRequestHandler):
    work: Path
    library: Path | None
    trainings: Path | None

    def do_GET(self) -> None:  # the name http.server calls
        if self.path in ("/", "/index.html"):
            body, kind = PAGE.read_bytes(), "text/html; charset=utf-8"
        elif self.path == "/api/state":
            body, kind = json.dumps(state(self.work, self.library, self.trainings), ensure_ascii=False).encode("utf-8"), "application/json"
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


def server(work: Path, port: int = 8765, library: Path | None = None, trainings: Path | None = None) -> ThreadingHTTPServer:
    paths = {"work": Path(work), "library": Path(library) if library else None, "trainings": Path(trainings) if trainings else None}
    handler = type("Handler", (_Handler,), paths)
    return ThreadingHTTPServer(("127.0.0.1", port), handler)


def serve(work: Path, port: int = 8765, library: Path | None = None, trainings: Path | None = None) -> None:
    httpd = server(work, port, library, trainings)
    print(f"bench dashboard: http://127.0.0.1:{httpd.server_address[1]}/ (reading {work})", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
