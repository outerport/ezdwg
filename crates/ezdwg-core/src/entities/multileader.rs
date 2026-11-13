//! MULTILEADER (AcDbMLeader) of R2010 and later.
//!
//! The data stream holds, in order: the class version, the leader roots
//! (each with its leader lines and their vertices), the annotation context
//! (scale, content base, the text or block content, the base plane), and the
//! entity's style overrides. Its strings (the text content and any colour
//! names) are in the object's string stream, in the order the fields name
//! them; its handles are in the handle stream and are not read here.

use crate::bit::BitReader;
use crate::core::error::{DwgError, ErrorKind};
use crate::core::result::Result;
use crate::entities::common::{
    parse_common_entity_header_r2010, parse_common_entity_header_r2013, string_stream_reader,
    CommonEntityHeader,
};

type Point3 = (f64, f64, f64);

/// Largest count of roots, lines, vertices or breaks a record may claim.
const MAX_ITEMS: u32 = 5000;

#[derive(Debug, Clone, PartialEq)]
pub struct MLeaderLine {
    pub vertices: Vec<Point3>,
    pub line_index: u32,
    pub line_type: u16,
    pub color_index: u16,
    pub true_color: Option<u32>,
    pub lineweight: i32,
    pub arrow_size: f64,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MLeaderRoot {
    pub last_leader_point: Option<Point3>,
    pub dogleg_vector: Option<Point3>,
    pub breaks: Vec<(Point3, Point3)>,
    pub branch_index: u32,
    pub dogleg_length: f64,
    pub lines: Vec<MLeaderLine>,
    pub attachment_direction: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MLeaderText {
    pub text: String,
    pub normal: Point3,
    pub location: Point3,
    pub direction: Point3,
    pub rotation: f64,
    pub width: f64,
    pub height: f64,
    pub line_spacing_factor: f64,
    pub line_spacing_style: u16,
    pub color_index: u16,
    pub true_color: Option<u32>,
    pub alignment: u16,
    pub flow_direction: u16,
    pub background_color_index: u16,
    pub background_true_color: Option<u32>,
    pub background_scale: f64,
    pub background_transparency: u32,
    pub has_background_fill: bool,
    pub has_background_mask_fill: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MLeaderBlock {
    pub normal: Point3,
    pub location: Point3,
    pub scale: Point3,
    pub rotation: f64,
    pub color_index: u16,
    pub transform: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiLeaderEntity {
    pub handle: u64,
    pub roots: Vec<MLeaderRoot>,
    pub scale: f64,
    pub content_base: Point3,
    pub text_height: f64,
    pub arrow_size: f64,
    pub landing_gap: f64,
    pub text_left_attachment: u16,
    pub text_right_attachment: u16,
    pub text_angle_type: u16,
    pub text_alignment: u16,
    pub text: Option<MLeaderText>,
    pub block: Option<MLeaderBlock>,
    pub base_point: Point3,
    pub base_direction: Point3,
    pub base_vertical: Point3,
    pub normal_reversed: bool,
    pub text_top_attachment: u16,
    pub text_bottom_attachment: u16,
    pub override_flags: u32,
    pub leader_type: u16,
    pub line_color_index: u16,
    pub line_true_color: Option<u32>,
    pub lineweight: i32,
    pub has_landing: bool,
    pub has_dogleg: bool,
    pub landing_distance: f64,
    pub style_arrow_size: f64,
    pub content_type: u16,
    pub style_text_left: u16,
    pub style_text_right: u16,
    pub style_text_angle_type: u16,
    pub style_text_alignment: u16,
    pub text_color_index: u16,
    pub text_true_color: Option<u32>,
    pub has_text_frame: bool,
    pub block_scale: Point3,
    pub block_rotation: f64,
    pub block_attachment: u16,
    pub annotative: bool,
    pub attachment_direction: u16,
}

/// A colour (`CMC`, R2004 and later): `BS` index, `BL` RGB with the colour
/// method in its top byte, `RC` flags; the flags say whether a colour name
/// and a book name follow in the string stream.
struct Color {
    index: u16,
    true_color: Option<u32>,
}

fn read_color(reader: &mut BitReader<'_>, strings: &mut Option<BitReader<'_>>) -> Result<Color> {
    let index = reader.read_bs()?;
    let rgb = reader.read_bl()?;
    let flags = reader.read_rc()?;
    if flags < 4 {
        for bit in [1u8, 2u8] {
            if flags & bit != 0 {
                if let Some(stream) = strings.as_mut() {
                    stream.read_tu()?;
                }
            }
        }
    }
    // Method 0xC2 is an RGB colour; 0xC3 an index and 0xC1/0xC0 by layer or block.
    let true_color = (rgb >> 24 == 0xC2).then_some(rgb & 0x00FF_FFFF);
    Ok(Color { index, true_color })
}

fn count(reader: &mut BitReader<'_>, what: &str) -> Result<u32> {
    let value = reader.read_bl()?;
    if value > MAX_ITEMS {
        return Err(DwgError::new(
            ErrorKind::Format,
            format!("MULTILEADER {what} count too large: {value}"),
        ));
    }
    Ok(value)
}

fn read_line(
    reader: &mut BitReader<'_>,
    strings: &mut Option<BitReader<'_>>,
) -> Result<MLeaderLine> {
    let points = count(reader, "vertex")?;
    let mut vertices = Vec::with_capacity(points as usize);
    for _ in 0..points {
        vertices.push(reader.read_3bd()?);
    }
    let breaks = count(reader, "line break")?;
    for _ in 0..breaks {
        reader.read_3bd()?;
        reader.read_3bd()?;
    }
    let line_index = reader.read_bl()?;
    let line_type = reader.read_bs()?;
    let color = read_color(reader, strings)?;
    let lineweight = reader.read_bl()? as i32;
    let arrow_size = reader.read_bd()?;
    let flags = reader.read_bl()?;
    Ok(MLeaderLine {
        vertices,
        line_index,
        line_type,
        color_index: color.index,
        true_color: color.true_color,
        lineweight,
        arrow_size,
        flags,
    })
}

fn read_root(
    reader: &mut BitReader<'_>,
    strings: &mut Option<BitReader<'_>>,
) -> Result<MLeaderRoot> {
    let has_last = reader.read_b()? != 0;
    let has_dogleg = reader.read_b()? != 0;
    let last_leader_point = if has_last {
        Some(reader.read_3bd()?)
    } else {
        None
    };
    let dogleg_vector = if has_dogleg {
        Some(reader.read_3bd()?)
    } else {
        None
    };
    let break_count = count(reader, "root break")?;
    let mut breaks = Vec::with_capacity(break_count as usize);
    for _ in 0..break_count {
        breaks.push((reader.read_3bd()?, reader.read_3bd()?));
    }
    let branch_index = reader.read_bl()?;
    let dogleg_length = reader.read_bd()?;
    let line_count = count(reader, "line")?;
    let mut lines = Vec::with_capacity(line_count as usize);
    for _ in 0..line_count {
        lines.push(read_line(reader, strings)?);
    }
    let attachment_direction = reader.read_bs()?;
    Ok(MLeaderRoot {
        last_leader_point,
        dogleg_vector,
        breaks,
        branch_index,
        dogleg_length,
        lines,
        attachment_direction,
    })
}

fn read_text(
    reader: &mut BitReader<'_>,
    strings: &mut Option<BitReader<'_>>,
) -> Result<MLeaderText> {
    let text = match strings.as_mut() {
        Some(stream) => stream.read_tu()?,
        None => String::new(),
    };
    let normal = reader.read_3bd()?;
    let location = reader.read_3bd()?;
    let direction = reader.read_3bd()?;
    let rotation = reader.read_bd()?;
    let width = reader.read_bd()?;
    let height = reader.read_bd()?;
    let line_spacing_factor = reader.read_bd()?;
    let line_spacing_style = reader.read_bs()?;
    let color = read_color(reader, strings)?;
    let alignment = reader.read_bs()?;
    let flow_direction = reader.read_bs()?;
    let background = read_color(reader, strings)?;
    let background_scale = reader.read_bd()?;
    let background_transparency = reader.read_bl()?;
    let has_background_fill = reader.read_b()? != 0;
    let has_background_mask_fill = reader.read_b()? != 0;
    let _column_type = reader.read_bs()?;
    let _height_auto = reader.read_b()?;
    let _column_width = reader.read_bd()?;
    let _column_gutter = reader.read_bd()?;
    let _column_flow_reversed = reader.read_b()?;
    let column_sizes = count(reader, "column size")?;
    for _ in 0..column_sizes {
        reader.read_bd()?;
    }
    let _word_break = reader.read_b()?;
    let _unknown = reader.read_b()?;
    Ok(MLeaderText {
        text,
        normal,
        location,
        direction,
        rotation,
        width,
        height,
        line_spacing_factor,
        line_spacing_style,
        color_index: color.index,
        true_color: color.true_color,
        alignment,
        flow_direction,
        background_color_index: background.index,
        background_true_color: background.true_color,
        background_scale,
        background_transparency,
        has_background_fill,
        has_background_mask_fill,
    })
}

fn read_block(
    reader: &mut BitReader<'_>,
    strings: &mut Option<BitReader<'_>>,
) -> Result<MLeaderBlock> {
    let normal = reader.read_3bd()?;
    let location = reader.read_3bd()?;
    let scale = reader.read_3bd()?;
    let rotation = reader.read_bd()?;
    let color = read_color(reader, strings)?;
    let mut transform = Vec::with_capacity(16);
    for _ in 0..16 {
        transform.push(reader.read_bd()?);
    }
    Ok(MLeaderBlock {
        normal,
        location,
        scale,
        rotation,
        color_index: color.index,
        transform,
    })
}

fn decode_with_header(
    reader: &mut BitReader<'_>,
    header: CommonEntityHeader,
    object_handle: u64,
) -> Result<MultiLeaderEntity> {
    // The string stream ends where the handle stream begins, the end of the object's data.
    let mut strings = string_stream_reader(reader, header.obj_size).map(|(stream, _)| stream);
    let _class_version = reader.read_bs()?;
    let root_count = count(reader, "root")?;
    let mut roots = Vec::with_capacity(root_count as usize);
    for _ in 0..root_count {
        roots.push(read_root(reader, &mut strings)?);
    }
    let scale = reader.read_bd()?;
    let content_base = reader.read_3bd()?;
    let text_height = reader.read_bd()?;
    let arrow_size = reader.read_bd()?;
    let landing_gap = reader.read_bd()?;
    let text_left_attachment = reader.read_bs()?;
    let text_right_attachment = reader.read_bs()?;
    let text_angle_type = reader.read_bs()?;
    let text_alignment = reader.read_bs()?;
    let has_text = reader.read_b()? != 0;
    let mut text = None;
    let mut block = None;
    if has_text {
        text = Some(read_text(reader, &mut strings)?);
    } else if reader.read_b()? != 0 {
        block = Some(read_block(reader, &mut strings)?);
    }
    let base_point = reader.read_3bd()?;
    let base_direction = reader.read_3bd()?;
    let base_vertical = reader.read_3bd()?;
    let normal_reversed = reader.read_b()? != 0;
    let text_top_attachment = reader.read_bs()?;
    let text_bottom_attachment = reader.read_bs()?;

    let override_flags = reader.read_bl()?;
    let leader_type = reader.read_bs()?;
    let line_color = read_color(reader, &mut strings)?;
    let lineweight = reader.read_bl()? as i32;
    let has_landing = reader.read_b()? != 0;
    let has_dogleg = reader.read_b()? != 0;
    let landing_distance = reader.read_bd()?;
    let style_arrow_size = reader.read_bd()?;
    let content_type = reader.read_bs()?;
    let style_text_left = reader.read_bs()?;
    let style_text_right = reader.read_bs()?;
    let style_text_angle_type = reader.read_bs()?;
    let style_text_alignment = reader.read_bs()?;
    let text_color = read_color(reader, &mut strings)?;
    let has_text_frame = reader.read_b()? != 0;
    let _block_color = read_color(reader, &mut strings)?;
    let block_scale = reader.read_3bd()?;
    let block_rotation = reader.read_bd()?;
    let block_attachment = reader.read_bs()?;
    let annotative = reader.read_b()? != 0;
    let attachment_direction = reader.read_bs()?;
    let _attachment_top = reader.read_bs()?;
    let _attachment_bottom = reader.read_bs()?;
    Ok(MultiLeaderEntity {
        handle: object_handle,
        roots,
        scale,
        content_base,
        text_height,
        arrow_size,
        landing_gap,
        text_left_attachment,
        text_right_attachment,
        text_angle_type,
        text_alignment,
        text,
        block,
        base_point,
        base_direction,
        base_vertical,
        normal_reversed,
        text_top_attachment,
        text_bottom_attachment,
        override_flags,
        leader_type,
        line_color_index: line_color.index,
        line_true_color: line_color.true_color,
        lineweight,
        has_landing,
        has_dogleg,
        landing_distance,
        style_arrow_size,
        content_type,
        style_text_left,
        style_text_right,
        style_text_angle_type,
        style_text_alignment,
        text_color_index: text_color.index,
        text_true_color: text_color.true_color,
        has_text_frame,
        block_scale,
        block_rotation,
        block_attachment,
        annotative,
        attachment_direction,
    })
}

/// A MULTILEADER of R2010; the reader stands after the object type.
pub fn decode_multileader_r2010(
    reader: &mut BitReader<'_>,
    object_data_end_bit: u32,
    object_handle: u64,
) -> Result<MultiLeaderEntity> {
    let header = parse_common_entity_header_r2010(reader, object_data_end_bit)?;
    decode_with_header(reader, header, object_handle)
}

/// A MULTILEADER of R2013 and later; the reader stands after the object type.
pub fn decode_multileader_r2013(
    reader: &mut BitReader<'_>,
    object_data_end_bit: u32,
    object_handle: u64,
) -> Result<MultiLeaderEntity> {
    let header = parse_common_entity_header_r2013(reader, object_data_end_bit)?;
    decode_with_header(reader, header, object_handle)
}
