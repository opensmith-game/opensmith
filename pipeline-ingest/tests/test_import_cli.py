"""What the app relies on to import songs: names from files, progress, JSON lines."""

import json
import subprocess
import sys

import pytest
from test_importers import make_gp5

from opensmith_ingest.naming import guess_title_artist
from opensmith_ingest.progress import Progress


@pytest.mark.parametrize(
    "stem, expected",
    [
        ("The Larsens - Night Drive (Official HD Video)", ("Night Drive", "The Larsens")),
        ("03. Iron Gate - Desert Road [Remastered]", ("Desert Road", "Iron Gate")),
        ("01 - Back On Stage", ("Back On Stage", None)),
        ("AB-CD – Thunderbolt (Lyrics)", ("Thunderbolt", "AB-CD")),
        ("sounds_like_garage_spirit", ("sounds like garage spirit", None)),
        ("4096 Overture", ("4096 Overture", None)),
        ("77 Reasons", ("77 Reasons", None)),
    ],
)
def test_title_and_artist_come_from_the_file_name(stem, expected):
    assert guess_title_artist(stem) == expected


def test_tags_keep_their_artist_unless_the_title_names_it():
    video = "The Larsens - Come Back Home (Official Music Video)"
    assert guess_title_artist(video, "TheLarsensVEVO") == ("Come Back Home", "The Larsens")
    assert guess_title_artist("Garage Anthem - Live at the Club", "Les Vautours") == ("Garage Anthem - Live at the Club", None)


def test_progress_only_moves_forward_and_ends_at_one():
    seen = []
    p = Progress(lambda stage, f: seen.append((stage, f)))
    p.stage("decode")
    p.stage("separate")
    for k in range(5):
        p.update(k / 4)
    p.update(0.5)  # a late report from inside a stage cannot move the bar back
    for stage in ("stems", "transcribe", "beats", "chart"):
        p.stage(stage)
    p.done()
    fractions = [f for _, f in seen]
    assert fractions == sorted(fractions)
    assert seen[-1] == ("done", 1.0)
    assert [f for s, f in seen if s == "separate"][-1] == pytest.approx(0.55)


def run_cli(*args):
    done = subprocess.run([sys.executable, "-m", "opensmith_ingest.cli", *args], capture_output=True, text=True, timeout=120)
    lines = [json.loads(line) for line in done.stdout.splitlines()]
    return done.returncode, lines


def test_an_import_reports_json_lines_only(tmp_path):
    tab = tmp_path / "riff.gp5"
    make_gp5(tab)
    code, lines = run_cli("guitarpro", str(tab), "-o", str(tmp_path / "song"), "--progress-json")
    assert code == 0
    assert lines[0]["stage"] == "chart"
    assert lines[-1]["done"] is True
    assert (tmp_path / "song" / "song_chart.json").exists()
    assert lines[-1]["chart"] == str(tmp_path / "song" / "song_chart.json")


def test_a_failed_import_says_why_in_json(tmp_path):
    bad = tmp_path / "not-a-song.mp3"
    bad.write_bytes(b"this is not audio")
    code, lines = run_cli("audio", str(bad), "-o", str(tmp_path / "out"), "--progress-json")
    assert code == 1
    assert lines[0] == {"stage": "decode", "fraction": 0.0}
    assert "ffmpeg could not read not-a-song.mp3" in lines[-1]["error"]


def test_the_song_picture_is_cropped_square(tmp_path):
    from opensmith_ingest.audio_io import extract_cover

    tone, picture, song = tmp_path / "tone.mp3", tmp_path / "thumb.png", tmp_path / "song.mp3"
    run = lambda *a: subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", *a], check=True)  # noqa: E731
    run("-f", "lavfi", "-i", "sine=frequency=440:duration=1", str(tone))
    run("-f", "lavfi", "-i", "color=c=red:s=320x180", "-frames:v", "1", str(picture))
    run("-i", str(tone), "-i", str(picture), "-map", "0", "-map", "1", "-c", "copy", "-disposition:v", "attached_pic", str(song))
    assert extract_cover(song, tmp_path / "cover.jpg")
    size = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0", str(tmp_path / "cover.jpg")],
        capture_output=True,
        text=True,
    ).stdout.strip()
    assert size == "512,512"
    assert not extract_cover(tone, tmp_path / "none.jpg"), "no picture in the file"
    assert not (tmp_path / "none.jpg").exists()
