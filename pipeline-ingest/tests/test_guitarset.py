"""Fingering versus real guitarists: GuitarSet hexaphonic annotations (CC BY 4.0).

The solver's weights were fitted on players 00-02; these checks use players 03-05 only.
The 39 MB annotation archive is downloaded once into ~/.cache/opensmith/guitarset.
"""

import pytest

from opensmith_ingest.benchmark import evaluate, fetch_annotations


@pytest.fixture(scope="module")
def held_out():
    try:
        directory = fetch_annotations()
    except OSError as e:  # offline
        pytest.skip(f"GuitarSet annotations unavailable: {e}")
    return [p for p in sorted(directory.glob("*.jams")) if p.name[:2] in {"03", "04", "05"}]


def test_solver_agrees_with_guitarists_on_held_out_players(held_out):
    score = evaluate(held_out)
    solo_n, solo_hits = score.by_style["solo"]
    comp_n, comp_hits = score.by_style["comp"]
    print(
        f"\n{score.notes} notes: {100 * score.accuracy:.1f}% same string as the guitarist "
        f"(solo {100 * solo_hits / solo_n:.1f}%, comp {100 * comp_hits / comp_n:.1f}%), "
        f"lowest-fret baseline {100 * score.baseline:.1f}%"
    )
    assert score.notes > 25_000
    assert score.accuracy > 0.68
    assert solo_hits / solo_n > 0.62
    assert comp_hits / comp_n > 0.68
    assert score.accuracy > score.baseline + 0.2
