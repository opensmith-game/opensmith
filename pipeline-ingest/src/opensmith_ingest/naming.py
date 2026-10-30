"""Song title and artist from a file name, for files without tags."""

from __future__ import annotations

import re

# "(Official HD Video)", "[Lyrics]", "(Audio)", "(Remastered 2011)"…
_NOISE = re.compile(
    r"\s*[\(\[][^\)\]]*\b(official|video|audio|lyrics?|clip|hd|hq|4k|remaster(?:ed)?|visuali[sz]er|explicit)\b[^\)\]]*[\)\]]",
    re.IGNORECASE,
)
# A leading track number followed by punctuation: "01 - ", "03. ", "7_", "12) ".
_TRACK = re.compile(r"^\d{1,3}\s*[.\-_)]\s*")
_SEPARATORS = (" - ", " – ", " — ")


def _key(name: str) -> str:
    return "".join(c for c in name.lower() if c.isalnum())


def guess_title_artist(stem: str, artist: str | None = None) -> tuple[str, str | None]:
    """'The Larsens - Night Drive (Official HD Video)' → ('Night Drive', 'The Larsens').

    `artist`, when the file has an artist tag, vetoes a split whose left part is not that
    artist ("Garage Anthem - Live at the Club" by Les Vautours stays whole; "The Larsens - …" by
    TheLarsensVEVO splits)."""
    name = _NOISE.sub("", stem.replace("_", " "))
    name = re.sub(r"\s+", " ", name).strip()
    name = _TRACK.sub("", name, count=1).strip()
    for sep in _SEPARATORS:
        if sep in name:
            left, right = (part.strip(" -–—") for part in name.split(sep, 1))
            same = artist is None or (_key(left) and (_key(left) in _key(artist) or _key(artist) in _key(left)))
            if left and right and same:
                return right, left
    return name or stem, None
