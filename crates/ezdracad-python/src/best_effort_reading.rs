//! Conversion of a best-effort drawing to Python rows; all decoding is in Rust.
use super::{limits, python_error, LimitValues};
use ezdracad_core::{
    BestEffortDrawing, BestEffortEntity, BestEffortGeometry, BestEffortLimits, BestEffortTable,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::path::PathBuf;

pub(crate) type BestEffortValues = (u64, u64, u64, u64, u64);

#[pyfunction]
pub(crate) fn best_effort_limit_defaults() -> BestEffortValues {
    let value = BestEffortLimits::default();
    (
        value.max_entities,
        value.max_depth,
        value.max_curve_points,
        value.max_block_definitions,
        value.max_diagnostics,
    )
}

fn best_effort_limits(values: BestEffortValues) -> BestEffortLimits {
    BestEffortLimits {
        max_entities: values.0,
        max_depth: values.1,
        max_curve_points: values.2,
        max_block_definitions: values.3,
        max_diagnostics: values.4,
    }
}

fn table_row(row: &Bound<'_, PyDict>, table: Option<BestEffortTable>) -> PyResult<()> {
    match table {
        Some(BestEffortTable::Root) => {
            row.set_item("table", "root")?;
            row.set_item("block_index", None::<u64>)
        }
        Some(BestEffortTable::Block(index)) => {
            row.set_item("table", "block")?;
            row.set_item("block_index", index)
        }
        None => {
            row.set_item("table", None::<&str>)?;
            row.set_item("block_index", None::<u64>)
        }
    }
}

fn fill_row<'py>(
    py: Python<'py>,
    fill: &Option<ezdracad_core::BestEffortFill>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    fill.as_ref()
        .map(|fill| {
            let row = PyDict::new(py);
            row.set_item("color", fill.color)?;
            // Numbers, not a byte string: a `Vec<u8>` would become `bytes`.
            row.set_item("rgb", fill.rgb.map(u16::from).to_vec())?;
            row.set_item("transparency", fill.transparency)?;
            row.set_item("outline", fill.outline)?;
            Ok(row)
        })
        .transpose()
}

fn geometry_row<'py>(
    py: Python<'py>,
    geometry: &BestEffortGeometry,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    match geometry {
        BestEffortGeometry::Point {
            position,
            point_type,
        } => {
            row.set_item("position", position)?;
            row.set_item("point_type", point_type)?;
        }
        BestEffortGeometry::Line {
            start,
            end,
            elevation,
            thickness,
        } => {
            row.set_item("start", start)?;
            row.set_item("end", end)?;
            row.set_item("elevation", elevation)?;
            row.set_item("thickness", thickness)?;
        }
        BestEffortGeometry::Line3D { start, end } => {
            row.set_item("start", start)?;
            row.set_item("end", end)?;
        }
        BestEffortGeometry::Circle {
            center,
            radius,
            elevation,
            fill,
        } => {
            row.set_item("center", center)?;
            row.set_item("radius", radius)?;
            row.set_item("elevation", elevation)?;
            row.set_item("fill", fill_row(py, fill)?)?;
        }
        BestEffortGeometry::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
            elevation,
        } => {
            row.set_item("center", center)?;
            row.set_item("radius", radius)?;
            row.set_item("start_degrees", start_degrees)?;
            row.set_item("end_degrees", end_degrees)?;
            row.set_item("elevation", elevation)?;
        }
        BestEffortGeometry::Ellipse {
            center,
            radius,
            rotation_degrees,
            fill,
        } => {
            row.set_item("center", center)?;
            row.set_item("radius", radius)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
            row.set_item("fill", fill_row(py, fill)?)?;
        }
        BestEffortGeometry::EllipticalArc {
            center,
            radius,
            rotation_degrees,
            start_degrees,
            end_degrees,
        } => {
            row.set_item("center", center)?;
            row.set_item("radius", radius)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
            row.set_item("start_degrees", start_degrees)?;
            row.set_item("end_degrees", end_degrees)?;
        }
        BestEffortGeometry::Text(text) => {
            row.set_item("position", text.position)?;
            row.set_item("elevation", text.elevation)?;
            row.set_item("text", &text.text)?;
            row.set_item("text_bytes", PyBytes::new(py, &text.text_bytes))?;
            row.set_item("height", text.height)?;
            row.set_item("width", text.width)?;
            row.set_item("spacing", text.spacing)?;
            row.set_item("rotation_degrees", text.rotation_degrees)?;
            row.set_item("slant_degrees", text.slant_degrees)?;
            row.set_item("origin", text.origin)?;
            row.set_item("flags", text.flags_raw)?;
            row.set_item("font_name", &text.font_name)?;
            row.set_item("charset", text.charset)?;
            row.set_item("family", text.family)?;
            row.set_item("numeric", text.numeric)?;
            row.set_item("number", text.number)?;
            row.set_item("unit", text.unit_raw)?;
            row.set_item("item", text.item_raw)?;
            row.set_item("output_size", text.output_size)?;
            row.set_item("format_codes", text.format_codes)?;
            row.set_item("lossy", text.lossy)?;
        }
        BestEffortGeometry::Insert {
            name,
            name_bytes,
            reference_number,
            position,
            elevation,
            scale,
            rotation_degrees,
        } => {
            row.set_item("name", name)?;
            row.set_item("name_bytes", PyBytes::new(py, name_bytes))?;
            row.set_item("reference_number", reference_number)?;
            row.set_item("position", position)?;
            row.set_item("elevation", elevation)?;
            row.set_item("scale", scale)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
        }
        BestEffortGeometry::Polyline {
            curve_kind,
            flags_raw,
            divisions,
            three_d,
            fill,
            curve_points,
        } => {
            row.set_item("curve_kind", curve_kind)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("closed", flags_raw & 0x01 != 0)?;
            row.set_item("divisions", divisions)?;
            row.set_item("three_d", three_d)?;
            row.set_item("fill", fill_row(py, fill)?)?;
            row.set_item("curve_points", curve_points.as_ref())?;
        }
        BestEffortGeometry::ExtendedPolygon {
            polygon_kind,
            flags_raw,
            fill,
            reference_number,
            hole_count,
            link_id,
            name,
        } => {
            row.set_item("polygon_kind", polygon_kind)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("fill", fill_row(py, fill)?)?;
            row.set_item("reference_number", reference_number)?;
            row.set_item("hole_count", hole_count)?;
            row.set_item("region_id", link_id)?;
            row.set_item("name", name)?;
        }
        BestEffortGeometry::Hatch {
            hatch_type,
            link_id,
            lines,
            pattern,
            gradation,
        } => {
            match gradation {
                Some(gradation) => {
                    let value = PyDict::new(py);
                    value.set_item("start", gradation.start)?;
                    value.set_item("end", gradation.end)?;
                    value.set_item("rgb_start", gradation.rgb_start.map(u16::from).to_vec())?;
                    value.set_item("rgb_end", gradation.rgb_end.map(u16::from).to_vec())?;
                    value.set_item("gradation_type", gradation.gradation_type)?;
                    row.set_item("gradation", value)?;
                }
                None => row.set_item("gradation", py.None())?,
            }
            row.set_item("hatch_type", hatch_type)?;
            row.set_item("region_id", link_id)?;
            let lines: Vec<_> = lines
                .iter()
                .map(|line| -> PyResult<_> {
                    let value = PyDict::new(py);
                    value.set_item("origin", line.origin)?;
                    value.set_item("pitch", line.pitch)?;
                    value.set_item("angle_degrees", line.angle_degrees)?;
                    value.set_item("color", line.color)?;
                    value.set_item("line_type", line.line_type)?;
                    value.set_item("width", line.width)?;
                    Ok(value)
                })
                .collect::<PyResult<_>>()?;
            row.set_item("lines", lines)?;
            match pattern {
                Some(pattern) => {
                    let value = PyDict::new(py);
                    value.set_item("name", &pattern.name)?;
                    value.set_item("origin", pattern.origin)?;
                    value.set_item("values", pattern.values.clone())?;
                    value.set_item("color", pattern.color)?;
                    value.set_item("line_type", pattern.line_type)?;
                    value.set_item("width", pattern.width)?;
                    row.set_item("pattern", value)?;
                }
                None => row.set_item("pattern", py.None())?,
            }
        }
        BestEffortGeometry::Dimension {
            dimension_type,
            flags_raw,
            flags2_raw,
            arrow_styles,
            text_position,
            line,
            arc,
            extension_origins,
            arrow_sizes,
            arrow_paper_sizes,
        } => {
            row.set_item("dimension_type", dimension_type)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("flags2", flags2_raw)?;
            row.set_item("arrow_styles", arrow_styles.map(u16::from).to_vec())?;
            row.set_item("text_position", text_position)?;
            row.set_item("line", line)?;
            match arc {
                Some(arc) => {
                    let value = PyDict::new(py);
                    value.set_item("center", arc.center)?;
                    value.set_item("radius", arc.radius)?;
                    value.set_item("start_degrees", arc.start_degrees)?;
                    value.set_item("end_degrees", arc.end_degrees)?;
                    row.set_item("arc", value)?;
                }
                None => row.set_item("arc", py.None())?,
            }
            row.set_item("extension_origins", extension_origins)?;
            row.set_item("arrow_sizes", arrow_sizes)?;
            row.set_item("arrow_paper_sizes", arrow_paper_sizes)?;
        }
        BestEffortGeometry::Leader {
            leader_type,
            flags_raw,
            flags2_raw,
            arrow_style,
            lines,
            arrow_size,
            arrow_paper_size,
        } => {
            row.set_item("leader_type", leader_type)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("flags2", flags2_raw)?;
            row.set_item("arrow_style", arrow_style)?;
            row.set_item("lines", lines)?;
            row.set_item("arrow_size", arrow_size)?;
            row.set_item("arrow_paper_size", arrow_paper_size)?;
        }
        BestEffortGeometry::Package {
            package_kind,
            flags_raw,
            reference_number,
        } => {
            row.set_item("package_kind", package_kind)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("reference_number", reference_number)?;
        }
        BestEffortGeometry::Arrowhead {
            position,
            size,
            angle_degrees,
            arrow_type,
            flags_raw,
        } => {
            row.set_item("position", position)?;
            row.set_item("size", size)?;
            row.set_item("angle_degrees", angle_degrees)?;
            row.set_item("arrow_type", arrow_type)?;
            row.set_item("flags", flags_raw)?;
            row.set_item("output_size", flags_raw & 0x80 != 0)?;
        }
        BestEffortGeometry::Image {
            file_name,
            directory,
            position,
            elevation,
            scale,
            size,
            rotation_degrees,
        } => {
            row.set_item("file_name", file_name)?;
            row.set_item("directory", directory)?;
            row.set_item("position", position)?;
            row.set_item("elevation", elevation)?;
            row.set_item("scale", scale)?;
            row.set_item("size", size)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
        }
        BestEffortGeometry::ExternalReference {
            symbol,
            file_name,
            directory,
            relative_directory,
            position,
            elevation,
            scale,
            rotation_degrees,
        } => {
            row.set_item("symbol", symbol)?;
            row.set_item("file_name", file_name)?;
            row.set_item("directory", directory)?;
            row.set_item("relative_directory", relative_directory)?;
            row.set_item("position", position)?;
            row.set_item("elevation", elevation)?;
            row.set_item("scale", scale)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
        }
        BestEffortGeometry::Layout {
            number,
            name,
            paper_type,
            paper_size,
        } => {
            row.set_item("number", number)?;
            row.set_item("name", name)?;
            row.set_item("paper_type", paper_type)?;
            row.set_item("paper_size", paper_size)?;
        }
        BestEffortGeometry::Viewport {
            flags_raw,
            paper_point,
            model_point,
            scale,
            rotation_degrees,
            window,
        } => {
            row.set_item("flags", flags_raw)?;
            row.set_item("paper_point", paper_point)?;
            row.set_item("model_point", model_point)?;
            row.set_item("scale", scale)?;
            row.set_item("rotation_degrees", rotation_degrees)?;
            match window {
                Some([center, half_size]) => {
                    row.set_item("center", center)?;
                    row.set_item("half_size", half_size)?;
                }
                None => {
                    row.set_item("center", py.None())?;
                    row.set_item("half_size", py.None())?;
                }
            }
        }
        BestEffortGeometry::Polygon3D { hole_count } => {
            row.set_item("hole_count", hole_count)?;
        }
        BestEffortGeometry::Camera { position, target }
        | BestEffortGeometry::Light { position, target } => {
            row.set_item("position", position)?;
            row.set_item("target", target)?;
        }
    }
    Ok(row)
}

fn entity_rows<'py>(
    py: Python<'py>,
    entities: &[BestEffortEntity],
) -> PyResult<Bound<'py, PyList>> {
    let rows = PyList::empty(py);
    for entity in entities {
        let row = PyDict::new(py);
        let attributes = &entity.attributes;
        row.set_item("type", entity.kind())?;
        row.set_item("tag", attributes.tag_raw)?;
        row.set_item("flags", attributes.flags_raw)?;
        row.set_item("layer", attributes.layer)?;
        row.set_item("segment", attributes.segment)?;
        row.set_item("color", attributes.color)?;
        row.set_item("line_type", attributes.line_type)?;
        row.set_item("width", attributes.width)?;
        row.set_item("material", attributes.material)?;
        row.set_item("group", attributes.group)?;
        row.set_item("link_id", attributes.link_id)?;
        table_row(&row, Some(entity.source.table))?;
        row.set_item("record_index", entity.source.record_index)?;
        row.set_item("payload_offset", entity.source.payload_offset)?;
        row.set_item("record_count", entity.source.record_count)?;
        row.set_item("data", geometry_row(py, &entity.geometry)?)?;
        row.set_item("children", entity_rows(py, &entity.children)?)?;
        rows.append(row)?;
    }
    Ok(rows)
}

fn drawing_row<'py>(py: Python<'py>, drawing: &BestEffortDrawing) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("schema_version", 1)?;
    row.set_item("format", drawing.format.as_str())?;
    row.set_item("marker", drawing.marker_raw)?;
    row.set_item("marker_observed", drawing.marker_observed)?;
    row.set_item("payload_size", drawing.payload_size)?;
    row.set_item("payload_sha256", &drawing.payload_sha256)?;
    row.set_item("record_count", drawing.record_count)?;
    match &drawing.settings {
        Some(settings) => {
            let value = PyDict::new(py);
            value.set_item("drawing_origin", settings.drawing_origin)?;
            value.set_item("drawing_scale", settings.drawing_scale)?;
            value.set_item("paper_size", settings.paper_size)?;
            value.set_item("paper_origin", settings.paper_origin)?;
            value.set_item("paper_type", settings.paper_type)?;
            value.set_item("paper_portrait", settings.paper_portrait)?;
            value.set_item("scale_ratio", settings.scale_ratio)?;
            value.set_item("comment", &settings.comment)?;
            value.set_item("width_by_color", settings.width_by_color)?;
            row.set_item("settings", value)?;
        }
        None => row.set_item("settings", py.None())?,
    }
    let layers: Vec<_> = drawing
        .layers
        .iter()
        .map(|layer| {
            (
                layer.index,
                layer.name.as_str(),
                layer.flags_raw,
                layer.hidden,
                layer.color,
                layer.line_type,
                layer.width,
            )
        })
        .collect();
    row.set_item("layers", layers)?;
    let colors: Vec<_> = drawing
        .colors
        .iter()
        .map(|color| (color.index, color.rgb.to_vec(), color.pen_width))
        .collect();
    row.set_item("colors", colors)?;
    let line_types: Vec<_> = drawing
        .line_types
        .iter()
        .map(|line_type| {
            (
                line_type.index,
                line_type.pen_style,
                line_type.pattern.clone(),
            )
        })
        .collect();
    row.set_item("line_types", line_types)?;
    let widths: Vec<_> = drawing
        .widths
        .iter()
        .map(|width| (width.index, width.width))
        .collect();
    row.set_item("widths", widths)?;
    let blocks = PyList::empty(py);
    for block in &drawing.blocks {
        let value = PyDict::new(py);
        value.set_item("index", block.index)?;
        value.set_item("name", &block.name)?;
        value.set_item("name_bytes", PyBytes::new(py, &block.name_bytes))?;
        value.set_item("header_offset", block.header_offset)?;
        value.set_item("entities", entity_rows(py, &block.entities)?)?;
        blocks.append(value)?;
    }
    row.set_item("blocks", blocks)?;
    row.set_item("entities", entity_rows(py, &drawing.entities)?)?;
    let properties = PyList::empty(py);
    for property in &drawing.properties {
        let value = PyDict::new(py);
        table_row(&value, Some(property.source.table))?;
        value.set_item("record_index", property.source.record_index)?;
        value.set_item("payload_offset", property.source.payload_offset)?;
        value.set_item("owner", &property.owner)?;
        value.set_item("name", &property.name)?;
        value.set_item("value", &property.value)?;
        value.set_item("lossy", property.lossy)?;
        properties.append(value)?;
    }
    row.set_item("properties", properties)?;
    let report = PyDict::new(py);
    report.set_item("lossy", drawing.report.lossy())?;
    report.set_item("decoded_counts", &drawing.report.decoded_counts)?;
    report.set_item("unsupported_counts", &drawing.report.unsupported_counts)?;
    report.set_item("failed_counts", &drawing.report.failed_counts)?;
    report.set_item("property_records", drawing.report.property_records)?;
    report.set_item("erased_objects", drawing.report.erased_objects)?;
    report.set_item("orphan_records", drawing.report.orphan_records)?;
    report.set_item("clamped_spans", drawing.report.clamped_spans)?;
    let diagnostics: Vec<_> = drawing
        .report
        .diagnostics
        .iter()
        .map(|diagnostic| -> PyResult<_> {
            let value = PyDict::new(py);
            value.set_item("code", diagnostic.code)?;
            value.set_item("message", &diagnostic.message)?;
            table_row(&value, diagnostic.table)?;
            value.set_item("record_index", diagnostic.record_index)?;
            value.set_item("tag", diagnostic.tag_raw)?;
            Ok(value)
        })
        .collect::<PyResult<_>>()?;
    report.set_item("diagnostics", diagnostics)?;
    report.set_item(
        "diagnostics_suppressed",
        drawing.report.diagnostics_suppressed,
    )?;
    row.set_item("report", report)?;
    Ok(row)
}

#[pyfunction]
pub(crate) fn read_best_effort_bytes<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    values: LimitValues,
    best_effort_values: BestEffortValues,
) -> PyResult<Bound<'py, PyDict>> {
    let parse_limits = limits(values);
    parse_limits
        .check_file_size(data.as_bytes().len() as u64)
        .map_err(|error| python_error(py, error))?;
    // The detached closure owns its bytes and holds no Python references.
    let owned = data.as_bytes().to_vec();
    let read_limits = best_effort_limits(best_effort_values);
    let drawing = py
        .detach(move || ezdracad_core::read_best_effort(&owned, &parse_limits, &read_limits))
        .map_err(|error| python_error(py, error))?;
    drawing_row(py, &drawing)
}

#[pyfunction]
pub(crate) fn read_best_effort_file<'py>(
    py: Python<'py>,
    path: PathBuf,
    values: LimitValues,
    best_effort_values: BestEffortValues,
) -> PyResult<Bound<'py, PyDict>> {
    let parse_limits = limits(values);
    let read_limits = best_effort_limits(best_effort_values);
    let drawing = py
        .detach(move || ezdracad_core::read_best_effort_file(&path, &parse_limits, &read_limits))
        .map_err(|error| python_error(py, error))?;
    drawing_row(py, &drawing)
}
