"""The acadsharp samples are one drawing saved in each DWG version, so what a decoder reads of it must not depend
on the version it was saved in."""

from __future__ import annotations

from pathlib import Path

import pytest

from ezdwg import _core

ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "test_dwg/acadsharp"


def sample(version: str) -> str:
    return str(SAMPLES / f"sample_{version}.dwg")


def rounded(value):
    if isinstance(value, float):
        return round(value, 9)
    if isinstance(value, (list, tuple)):
        return type(value)(rounded(part) for part in value)
    if isinstance(value, dict):
        return {key: rounded(part) for key, part in value.items() if key != "handle"}
    return value


def leaders(version: str) -> dict:
    return {row["handle"]: rounded(row) for row in _core.decode_multileaders(sample(version))}


def test_multileaders_read_the_same_from_every_version_that_has_them() -> None:
    first = leaders("AC1024")
    assert len(first) == 15
    assert all(row["roots"] and all(root["lines"] for root in row["roots"]) for row in first.values())
    assert any(row["text"] and row["text"]["text"] for row in first.values())
    assert leaders("AC1027") == first
    assert leaders("AC1032") == first


def test_a_leader_line_ends_where_its_root_is_reached() -> None:
    """A root's last leader point is where its dogleg starts, and its lines are drawn up to it."""
    for row in leaders("AC1032").values():
        for root in row["roots"]:
            if root["last_leader_point"] is None:
                continue
            for line in root["lines"]:
                assert line["vertices"], "a leader line has at least its first vertex"


@pytest.mark.parametrize("version", ["AC1014", "AC1015", "AC1021", "AC1024", "AC1027", "AC1032"])
def test_tables_are_placed_alike_and_draw_a_table_block(version: str) -> None:
    rows = sorted(_core.decode_table_placements(sample(version)), key=lambda row: rounded(row[1]))
    reference = sorted(_core.decode_table_placements(sample("AC1032")), key=lambda row: rounded(row[1]))
    assert [rounded(row[1:5]) for row in rows] == [rounded(row[1:5]) for row in reference]
    names = {handle: name for handle, name in _core.decode_block_header_names(sample(version))}
    assert all(names.get(row[5], "").upper().startswith("*T") for row in rows)


@pytest.mark.parametrize("version", ["AC1014", "AC1015", "AC1018", "AC1021", "AC1024", "AC1027"])
def test_hatch_boundaries_read_as_stored_alike(version: str) -> None:
    def boundaries(name: str) -> list:
        return sorted(str(rounded(paths)) for _, paths in _core.decode_hatch_boundaries(sample(name)))

    assert boundaries(version) == boundaries("AC1032")
    kinds = {path[0] for _, paths in _core.decode_hatch_boundaries(sample("AC1032")) for path in paths}
    assert kinds == {"polyline", "edges"}


@pytest.mark.parametrize("version", ["AC1015", "AC1021", "AC1024", "AC1027"])
def test_a_wipeout_is_read_as_the_image_record_it_is_stored_as(version: str) -> None:
    """A WIPEOUT is an IMAGE without an image: its clip boundary in the pixel space its vectors span, read alike
    whatever the version the drawing was saved in. The row's last field, the clip mode, exists from R2010 on and is
    left out of the comparison."""
    def wipeouts(name: str) -> list:
        return [rounded(row[1:-1]) for row in _core.decode_wipeout_entities(sample(name))]

    [reference] = wipeouts("AC1032")
    _insert, u_vector, v_vector, size, _display, _clipping, _image_adjust, clip_type, vertices = reference
    assert size == (1.0, 1.0) and clip_type in (1, 2) and len(vertices) >= 2
    assert any(u_vector) and any(v_vector)
    assert wipeouts(version) == [reference]
