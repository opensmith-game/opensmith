"""Audio decoding (ffmpeg, any format), resampling (soxr) and WAV writing (soundfile)."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf
import soxr

ENGINE_RATE = 48_000


def decode(path: Path, rate: int, channels: int = 2) -> np.ndarray:
    """Decodes any file ffmpeg reads into float32 `(channels, samples)` at `rate`."""
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required to decode audio (brew install ffmpeg)")
    cmd = [ffmpeg, "-nostdin", "-v", "error", "-i", str(path), "-f", "f32le", "-acodec", "pcm_f32le", "-ac", str(channels), "-ar", str(rate), "-"]
    done = subprocess.run(cmd, capture_output=True)
    if done.returncode != 0:
        reason = done.stderr.decode(errors="replace").strip().splitlines()
        raise RuntimeError(f"ffmpeg could not read {Path(path).name}: {reason[-1] if reason else 'unknown error'}")
    audio = np.frombuffer(done.stdout, dtype=np.float32)
    return audio.reshape(-1, channels).T.copy()


def tags(path: Path) -> dict[str, str]:
    """Metadata tags of a song file (title, artist, album…), lower-case keys; {} if none."""
    ffprobe = shutil.which("ffprobe")
    if ffprobe is None:
        return {}
    cmd = [ffprobe, "-v", "error", "-show_entries", "format_tags", "-of", "json", str(path)]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=30, check=True).stdout
        found = json.loads(out).get("format", {}).get("tags", {})
    except (OSError, subprocess.SubprocessError, ValueError):
        return {}
    return {k.lower(): v.strip() for k, v in found.items() if isinstance(v, str) and v.strip()}


def extract_cover(path: Path, dst: Path, size: int = 512) -> bool:
    """The song file's embedded picture (album art, video thumbnail), centre-cropped square to
    `size` pixels as JPEG. False when there is none."""
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        return False
    crop = f"crop='min(iw,ih)':'min(iw,ih)',scale={size}:{size}"
    cmd = [ffmpeg, "-nostdin", "-v", "error", "-y", "-i", str(path), "-an", "-frames:v", "1", "-vf", crop, str(dst)]
    done = subprocess.run(cmd, capture_output=True)
    if done.returncode != 0 or not dst.exists() or dst.stat().st_size == 0:
        dst.unlink(missing_ok=True)
        return False
    return True


def resample(audio: np.ndarray, src: int, dst: int) -> np.ndarray:
    """`(channels, samples)` resampled with soxr's high-quality filter."""
    if src == dst:
        return audio
    return soxr.resample(audio.T, src, dst, quality="HQ").T.astype(np.float32)


def write_wav(path: Path, audio: np.ndarray, rate: int, *, pcm16: bool = False) -> None:
    """Writes `(channels, samples)` or mono `(samples,)` as 32-bit float WAV, or 16-bit for a
    finished mix (half the size: a song's library is many of them)."""
    data = audio.T if audio.ndim == 2 else audio
    if pcm16:
        sf.write(str(path), np.clip(data, -1.0, 1.0), rate, subtype="PCM_16")
    else:
        sf.write(str(path), data, rate, subtype="FLOAT")


def encode_mp3(src: Path, dst: Path, bitrate: str = "192k") -> None:
    """Encodes a WAV to MP3 with ffmpeg (test material)."""
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", str(src), "-b:a", bitrate, str(dst)], check=True)
