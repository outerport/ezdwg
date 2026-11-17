from __future__ import annotations

from pathlib import Path
from typing import Iterator

import pytest

import ezdwg
from ezdwg import raw


ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "test_dwg"


def _iter_dxf_entities(path: Path) -> Iterator[dict[str, object]]:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    section_name: str | None = None
    expect_section_name = False
    current_entity: dict[str, object] | None = None

    for i in range(0, len(lines) - 1, 2):
        code = lines[i].strip()
        value = lines[i + 1].strip()

        if code == "0":
            if current_entity is not None and section_name == "ENTITIES":
                yield current_entity
                current_entity = None

            if value == "SECTION":
                expect_section_name = True
                continue

            if value == "ENDSEC":
                section_name = None
                continue

            if section_name == "ENTITIES":
                current_entity = {"type": value, "groups": []}
            continue

        if expect_section_name and code == "2":
            section_name = value
            expect_section_name = False
            continue

        if section_name == "ENTITIES" and current_entity is not None:
            groups = current_entity["groups"]
            assert isinstance(groups, list)
            groups.append((code, value))

    if current_entity is not None and section_name == "ENTITIES":
        yield current_entity


def _dxf_entities_of_type(path: Path, entity_type: str) -> list[dict[str, object]]:
    return [entity for entity in _iter_dxf_entities(path) if entity["type"] == entity_type]


def _group_str(entity: dict[str, object], code: str, default: str = "") -> str:
    groups = entity["groups"]
    assert isinstance(groups, list)
    for group_code, raw_value in groups:
        if group_code == code:
            return raw_value
    return default


def test_text_counts_match_paired_dxf() -> None:
    for stem in ["text_2000", "text_2004"]:
        dxf_count = len(_dxf_entities_of_type(SAMPLES / f"{stem}.dxf", "TEXT"))
        rows = raw.decode_text_entities(str(SAMPLES / f"{stem}.dwg"))
        assert dxf_count == 1
        assert len(rows) == dxf_count


def test_mtext_counts_match_paired_dxf() -> None:
    for stem in ["mtext_2000", "mtext_2004"]:
        dxf_count = len(_dxf_entities_of_type(SAMPLES / f"{stem}.dxf", "MTEXT"))
        rows = raw.decode_mtext_entities(str(SAMPLES / f"{stem}.dwg"))
        assert dxf_count == 1
        assert len(rows) == dxf_count


def test_text_string_matches_paired_dxf() -> None:
    for stem in ["text_2000", "text_2004"]:
        dxf_text = _group_str(_dxf_entities_of_type(SAMPLES / f"{stem}.dxf", "TEXT")[0], "1")
        entities = list(ezdwg.read(str(SAMPLES / f"{stem}.dwg")).modelspace().query("TEXT"))
        assert len(entities) == 1
        assert entities[0].dxf["text"] == dxf_text


def test_mtext_string_matches_paired_dxf() -> None:
    for stem in ["mtext_2000", "mtext_2004"]:
        dxf_text = _group_str(_dxf_entities_of_type(SAMPLES / f"{stem}.dxf", "MTEXT")[0], "1")
        entities = list(ezdwg.read(str(SAMPLES / f"{stem}.dwg")).modelspace().query("MTEXT"))
        assert len(entities) == 1
        # High-level API returns normalized plain text.
        assert dxf_text in entities[0].dxf["text"] or entities[0].dxf["text"] in dxf_text


def test_mtext_plain_text_skips_the_height_caxa_writes_in_a_stack() -> None:
    from ezdwg.document import _decode_mtext_plain_text

    # CAXA writes the height of the stacked characters behind \S.
    assert _decode_mtext_plain_text("m\\S\\H0.7x;2^;") == "m2/"
    assert _decode_mtext_plain_text("D\\S\\H0.7x;+0.1^-0.2;") == "D+0.1/-0.2"
    # The AutoCAD stack is read as before.
    assert _decode_mtext_plain_text("\\S1^2;x") == "1/2x"
    assert _decode_mtext_plain_text("{\\H0.7x;\\S+0.1^-0.2;}A") == "+0.1/-0.2A"


def test_mtext_extents_are_the_drawn_text_as_autocad_stored_it() -> None:
    # The width and height AutoCAD stored for each text as drawn (DXF group codes 42 and 43), as the ODA File
    # Converter writes them; the reference column width (``rect_width``) is wider than the text on these.
    stored = {0x3EC: (13.49522510231924, 2.944292860391087), 0x779: (15.26057298772169, 204.3497044110957)}
    for stem in ["sample_AC1015", "sample_AC1018", "sample_AC1032"]:
        path = str(SAMPLES / "acadsharp" / f"{stem}.dwg")
        rows = {row[0]: row for row in raw.decode_mtext_entities(path)}
        entities = {
            entity.handle: entity
            for entity in ezdwg.read(path).modelspace().query("MTEXT")
        }
        for handle, (width, height) in stored.items():
            assert rows[handle][11] == pytest.approx((width, height), abs=1e-9)
            assert entities[handle].dxf["extents_width"] == pytest.approx(width, abs=1e-9)
            assert entities[handle].dxf["extents_height"] == pytest.approx(height, abs=1e-9)


def test_a_multi_line_attribute_keeps_its_value_content_and_style() -> None:
    # The value stored in front of the embedded MTEXT (empty here, as the ODA File Converter writes it), the
    # MTEXT's content as stored, and the attribute's own text style, read like a single-line attribute's.
    path = str(SAMPLES / "acadsharp" / "sample_AC1032.dwg")
    document = ezdwg.read(path)
    rows = {
        int(entity.handle): entity.dxf
        for layout in (document.modelspace(), document.entities())
        for entity in layout.query("ATTRIB ATTDEF")
    }
    attribute = rows[0x79D]
    assert attribute["multi_line_value"] == ""
    assert attribute["multi_line_content"] == "my multi line text for the attrrib"
    assert attribute["style_handle"] is not None
    assert rows[0x796]["multi_line_content"] == ""
    single = [dxf for dxf in rows.values() if dxf.get("multi_line_value") is None]
    assert single and all(dxf.get("multi_line_content") is None for dxf in single)


@pytest.mark.parametrize("stem", ["sample_AC1021", "sample_AC1024", "sample_AC1027", "sample_AC1032"])
def test_a_hatch_keeps_its_pattern_name_from_the_string_stream(stem: str) -> None:
    # The pattern names the ODA File Converter writes for the same drawing.
    document = ezdwg.read(str(SAMPLES / "acadsharp" / f"{stem}.dwg"))
    names = {
        entity.dxf["pattern_name"]
        for layout in (document.modelspace(), document.entities())
        for entity in layout.query("HATCH")
        if not entity.dxf.get("solid_fill")
    }
    assert names == {"ANSI31", "AR-PARQ1", "HSBDR3E0", "HSHP21E0", "HSHP31E0", "HVEGE100"}


def test_an_object_entitys_common_properties_are_read_by_handle() -> None:
    path = str(SAMPLES / "acadsharp" / "sample_AC1032.dwg")
    [wipeout] = raw.decode_wipeout_entities(path)
    assert raw.decode_object_entity_properties(path, [wipeout[0], 0xFFFFFF]) == [
        (wipeout[0], 256, None, False, -1, 1.0)
    ]
