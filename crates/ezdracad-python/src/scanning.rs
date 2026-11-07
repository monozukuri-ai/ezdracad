use std::path::PathBuf;

use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::{limits, mps_row, python_error, LimitValues};

type RecordRow = (u64, u64, u16);
type ObjectSpanRow = (u64, u64, u16, Py<PyBytes>, u64, u64);
type DrawingSettingsRow = (u64, Py<PyBytes>, [f64; 2], f64, f64);
type TextSizeUnitRow = (u64, Py<PyBytes>, u16, &'static str);
type LineRow = (u64, u64, [f64; 2], [f64; 2]);
type LineRecordRow = (u64, u64, u16, Py<PyBytes>, [f64; 2], [f64; 2], f64, f64);
type PackageReferenceRow = (
    u64,
    u64,
    u64,
    u64,
    Py<PyBytes>,
    Py<PyBytes>,
    Py<PyBytes>,
    [f64; 2],
    Py<PyBytes>,
    [f64; 6],
);

#[pyclass(name = "_RawHandle", frozen, module = "ezdracad._core")]
pub(crate) struct RawHandle {
    pub(crate) inner: ezdracad_core::RawDocument,
}

impl RawHandle {
    fn window(&self, py: Python<'_>, start: u64, count: u64) -> PyResult<std::ops::Range<u64>> {
        if count > ezdracad_core::MAX_RECORDS_PER_PAGE {
            return Err(python_error(
                py,
                ezdracad_core::DracadError::LimitExceeded {
                    resource: "records_per_page",
                    limit: ezdracad_core::MAX_RECORDS_PER_PAGE,
                    actual: count,
                },
            ));
        }
        let total = self.inner.header().record_count;
        if start > total {
            return Err(PyIndexError::new_err("record start exceeds record count"));
        }
        Ok(start..start.saturating_add(count).min(total))
    }
}

#[pymethods]
impl RawHandle {
    fn placed_block_lines_3d<'py>(
        &self,
        py: Python<'py>,
        index: u64,
        profile: &str,
        max_lines: u64,
    ) -> PyResult<Bound<'py, PyDict>> {
        let profile = ezdracad_core::Block3DProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let value = py
            .detach(|| self.inner.placed_block_lines_3d(index, profile, max_lines))
            .map_err(|error| python_error(py, error))?;
        super::block3d_scanning::selection_row(py, value)
    }

    fn world_input_3d_context(
        &self,
        py: Python<'_>,
        profile: &str,
    ) -> PyResult<super::world_input3d_scanning::WorldInput3DHandle> {
        let profile = ezdracad_core::WorldInput3DProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let inner = py
            .detach(|| ezdracad_core::world_input_3d_context(&self.inner, profile))
            .map_err(|error| python_error(py, error))?;
        Ok(super::world_input3d_scanning::WorldInput3DHandle { inner })
    }

    fn dimension_scale<'py>(
        &self,
        py: Python<'py>,
        index: u64,
        profile: &str,
    ) -> PyResult<Bound<'py, PyDict>> {
        let profile = ezdracad_core::DimensionScaleProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let value = py
            .detach(|| self.inner.dimension_scale(index, profile))
            .map_err(|error| python_error(py, error))?;
        super::dimension_scale_scanning::row(py, value)
    }

    fn workplane_context(
        &self,
        py: Python<'_>,
        profile: &str,
    ) -> PyResult<super::workplane_scanning::WorkplaneHandle> {
        let profile = ezdracad_core::WorkplaneProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let inner = py
            .detach(|| ezdracad_core::workplane_context(&self.inner, profile))
            .map_err(|error| python_error(py, error))?;
        Ok(super::workplane_scanning::WorkplaneHandle { inner })
    }

    fn coordinate_input_unit(
        &self,
        py: Python<'_>,
    ) -> PyResult<super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle> {
        let inner = self
            .inner
            .coordinate_input_unit()
            .map_err(|error| python_error(py, error))?;
        Ok(super::coordinate_input_units_scanning::NativeCoordinateInputUnitHandle { inner })
    }

    fn coordinate_space_context(
        &self,
        py: Python<'_>,
        profile: &str,
    ) -> PyResult<super::coordinate_space_scanning::CoordinateSpaceHandle> {
        let profile = ezdracad_core::CoordinateSpaceProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let inner = py
            .detach(|| ezdracad_core::coordinate_space_context(&self.inner, profile))
            .map_err(|error| python_error(py, error))?;
        Ok(super::coordinate_space_scanning::CoordinateSpaceHandle { inner })
    }

    // This private FFI signature carries both named profiles and the three
    // independent limits; the core API groups the latter as SceneLimits.
    #[allow(clippy::too_many_arguments)]
    fn qualified_lines_in_coordinate_space<'py>(
        &self,
        py: Python<'py>,
        index: u64,
        line_profile: &str,
        space_profile: &str,
        frame: &str,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Bound<'py, PyDict>> {
        let line_profile = ezdracad_core::Line2DProfile::from_id(line_profile)
            .map_err(|error| python_error(py, error))?;
        let space_profile = ezdracad_core::CoordinateSpaceProfile::from_id(space_profile)
            .map_err(|error| python_error(py, error))?;
        let frame = ezdracad_core::CoordinateFrame::from_id(frame)
            .map_err(|error| python_error(py, error))?;
        let limits = ezdracad_core::SceneLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let selection = py
            .detach(|| {
                ezdracad_core::qualify_lines_in_coordinate_space(
                    &self.inner,
                    index,
                    line_profile,
                    space_profile,
                    frame,
                    &limits,
                )
            })
            .map_err(|error| python_error(py, error))?;
        super::coordinate_space_scanning::selection_row(py, selection)
    }

    fn unit_resolution(
        &self,
        py: Python<'_>,
    ) -> PyResult<super::unit_resolution_scanning::UnitResolutionHandle> {
        let inner = self
            .inner
            .unit_resolution()
            .map_err(|error| python_error(py, error))?;
        Ok(super::unit_resolution_scanning::UnitResolutionHandle { inner })
    }

    fn qualified_lines<'py>(
        &self,
        py: Python<'py>,
        index: u64,
        profile: &str,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Bound<'py, PyDict>> {
        let profile = ezdracad_core::Line2DProfile::from_id(profile)
            .map_err(|error| python_error(py, error))?;
        let limits = ezdracad_core::SceneLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let value = py
            .detach(|| self.inner.qualified_lines(index, profile, &limits))
            .map_err(|error| python_error(py, error))?;
        super::qualification_scanning::selection_row(py, value)
    }

    fn origin_units_code(
        &self,
        py: Python<'_>,
    ) -> PyResult<super::origin_units_scanning::OriginUnitsRow> {
        let value = self
            .inner
            .origin_units_code()
            .map_err(|error| python_error(py, error))?;
        Ok(super::origin_units_scanning::row(py, value))
    }

    fn print_settings(
        &self,
        py: Python<'_>,
    ) -> PyResult<super::print_settings_scanning::PrintSettingsHandle> {
        let inner = self
            .inner
            .print_settings()
            .map_err(|error| python_error(py, error))?;
        Ok(super::print_settings_scanning::PrintSettingsHandle { inner })
    }

    fn scene_lines(
        &self,
        py: Python<'_>,
        index: u64,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Vec<Py<PyDict>>> {
        let limits = ezdracad_core::SceneLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let lines = py
            .detach(|| self.inner.scene_lines(index, &limits))
            .map_err(|error| python_error(py, error))?;
        super::scene_scanning::rows(py, lines)
    }

    fn scene_lines_with_inline_scopes(
        &self,
        py: Python<'_>,
        index: u64,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Vec<Py<PyDict>>> {
        let limits = ezdracad_core::SceneLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let lines = py
            .detach(|| self.inner.scene_lines_with_inline_scopes(index, &limits))
            .map_err(|error| python_error(py, error))?;
        super::scene_scanning::rows(py, lines)
    }

    fn coordinate_measurement_context(
        &self,
        py: Python<'_>,
    ) -> PyResult<super::coordinate_measurement_scanning::CoordinateMeasurementHandle> {
        let inner = self
            .inner
            .coordinate_measurement_context()
            .map_err(|error| python_error(py, error))?;
        Ok(super::coordinate_measurement_scanning::CoordinateMeasurementHandle { inner })
    }

    fn effective_line_style<'py>(
        &self,
        py: Python<'py>,
        index: u64,
    ) -> PyResult<Bound<'py, PyDict>> {
        let value = py
            .detach(|| self.inner.effective_line_style(index))
            .map_err(|error| python_error(py, error))?;
        super::effective_style_scanning::line_style_row(py, value)
    }

    fn record_attributes(
        &self,
        py: Python<'_>,
        index: u64,
    ) -> PyResult<super::record_attribute_scanning::AttributeRow> {
        let value = self
            .inner
            .record_attributes(index)
            .map_err(|error| python_error(py, error))?
            .ok_or_else(|| PyIndexError::new_err("record index out of range"))?;
        Ok(super::record_attribute_scanning::attribute_row(py, value))
    }

    fn attributed_line_records(
        &self,
        py: Python<'_>,
        start: u64,
        count: u64,
    ) -> PyResult<Vec<super::record_attribute_scanning::AttributedLineRow>> {
        let window = self.window(py, start, count)?;
        let lines = py
            .detach(|| {
                let mut lines = Vec::new();
                for index in window {
                    if let Some(line) = self.inner.attributed_line_record(index)? {
                        lines.push(line);
                    }
                }
                Ok::<_, ezdracad_core::DracadError>(lines)
            })
            .map_err(|error| python_error(py, error))?;
        Ok(lines
            .into_iter()
            .map(|line| super::record_attribute_scanning::line_row(py, line))
            .collect())
    }

    fn style_tables(&self, py: Python<'_>) -> PyResult<super::style_scanning::StyleHandle> {
        let inner = py
            .detach(|| ezdracad_core::RawStyleTables::parse(&self.inner))
            .map_err(|error| python_error(py, error))?;
        Ok(super::style_scanning::StyleHandle { inner })
    }
    fn inline_package_lines(
        &self,
        py: Python<'_>,
        index: u64,
        max_depth: u64,
        max_nodes: u64,
        max_lines: u64,
    ) -> PyResult<Vec<super::inline_scanning::InlineLineRow>> {
        let limits = ezdracad_core::InlinePackageLimits {
            max_depth,
            max_nodes,
            max_lines,
        };
        let lines = py
            .detach(|| self.inner.inline_package_lines_with_limits(index, &limits))
            .map_err(|error| python_error(py, error))?;
        Ok(super::inline_scanning::rows(py, lines))
    }

    fn drawing_settings(&self, py: Python<'_>) -> PyResult<DrawingSettingsRow> {
        let settings = self
            .inner
            .drawing_settings()
            .map_err(|error| python_error(py, error))?;
        Ok((
            settings.payload_offset,
            PyBytes::new(py, &settings.header_bytes).unbind(),
            settings.drawing_origin,
            settings.paper_scale_factor,
            settings.drawing_scale,
        ))
    }

    fn text_size_unit(&self, py: Python<'_>) -> PyResult<TextSizeUnitRow> {
        let info = self
            .inner
            .text_size_unit()
            .map_err(|error| python_error(py, error))?;
        Ok((
            info.payload_offset,
            PyBytes::new(py, &info.selector_bytes).unbind(),
            info.raw_selector,
            info.unit.as_str(),
        ))
    }

    fn block_library(
        &self,
        py: Python<'_>,
        max_definitions: u64,
    ) -> PyResult<super::block_scanning::BlockHandle> {
        let inner = py
            .detach(|| self.inner.block_library_with_limit(max_definitions))
            .map_err(|error| python_error(py, error))?;
        Ok(super::block_scanning::BlockHandle { inner })
    }
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let row = PyDict::new(py);
        let h = self.inner.header();
        let header = PyDict::new(py);
        header.set_item("profile", h.profile.as_str())?;
        header.set_item("marker_raw", h.marker_raw)?;
        header.set_item("record_start", h.record_start)?;
        header.set_item("record_end", h.record_end)?;
        header.set_item("record_count", h.record_count)?;
        header.set_item(
            "declared_nonterminal_records",
            h.declared_nonterminal_records,
        )?;
        header.set_item("payload_end", h.payload_end)?;
        header.set_item("payload_end_field_raw", h.payload_end_field_raw)?;
        header.set_item("payload_end_basis", h.payload_end_basis.as_str())?;
        header.set_item("section_offsets", h.section_offsets)?;
        header.set_item("optional_offsets", h.optional_offsets)?;
        row.set_item("header", header)?;
        row.set_item("payload_size", self.inner.contents().len())?;
        row.set_item("sha256", self.inner.sha256())?;
        row.set_item("line_candidate_count", self.inner.line_candidate_count())?;
        row.set_item("opaque_record_count", self.inner.opaque_record_count())?;
        row.set_item("tag_counts", self.inner.tag_counts())?;
        let source = self
            .inner
            .container()
            .ok_or_else(|| PyRuntimeError::new_err("missing input source"))?;
        row.set_item("file_size", source.file_size)?;
        row.set_item("format", source.format.as_str())?;
        row.set_item(
            "mps",
            source
                .mps
                .as_ref()
                .map(|mps| mps_row(py, mps))
                .transpose()?,
        )?;
        let payload = source
            .payload
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("missing drawing payload metadata"))?;
        row.set_item("stream_path", &payload.stream_path)?;
        row.set_item("offset_domain", payload.offset_domain())?;
        let diagnostics: Vec<_> = self
            .inner
            .diagnostics()
            .iter()
            .map(|item| (item.code, item.message.as_str()))
            .collect();
        row.set_item("diagnostics", diagnostics)?;
        Ok(row)
    }

    fn contents<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.contents())
    }

    fn records(&self, py: Python<'_>, start: u64, count: u64) -> PyResult<Vec<RecordRow>> {
        Ok(self
            .window(py, start, count)?
            .filter_map(|index| self.inner.record(index))
            .map(|record| (record.index, record.payload_offset, record.tag))
            .collect())
    }

    fn object_spans(&self, py: Python<'_>, max_objects: u64) -> PyResult<Vec<ObjectSpanRow>> {
        let spans = py
            .detach(|| {
                let mut spans = Vec::new();
                for decoded in self.inner.object_spans() {
                    let span = decoded?;
                    let actual = spans.len() as u64 + 1;
                    if actual > max_objects {
                        return Err(ezdracad_core::DracadError::LimitExceeded {
                            resource: "object_spans",
                            limit: max_objects,
                            actual,
                        });
                    }
                    spans.push(span);
                }
                Ok(spans)
            })
            .map_err(|error| python_error(py, error))?;
        Ok(spans
            .into_iter()
            .map(|span| {
                (
                    span.record_index,
                    span.payload_offset,
                    span.tag_raw,
                    PyBytes::new(py, &span.header_bytes).unbind(),
                    span.record_count,
                    span.end_record_index,
                )
            })
            .collect())
    }

    fn record_bytes<'py>(&self, py: Python<'py>, index: u64) -> PyResult<Bound<'py, PyBytes>> {
        let record = self
            .inner
            .record(index)
            .ok_or_else(|| PyIndexError::new_err("record index out of range"))?;
        Ok(PyBytes::new(py, record.bytes()))
    }

    fn line_candidates(&self, py: Python<'_>, start: u64, count: u64) -> PyResult<Vec<LineRow>> {
        Ok(self
            .window(py, start, count)?
            .filter_map(|index| self.inner.record(index))
            .filter_map(|record| record.line_candidate())
            .map(|line| (line.record_index, line.payload_offset, line.start, line.end))
            .collect())
    }

    fn line_records(&self, py: Python<'_>, start: u64, count: u64) -> PyResult<Vec<LineRecordRow>> {
        let mut rows = Vec::new();
        for index in self.window(py, start, count)? {
            let record = self
                .inner
                .record(index)
                .ok_or_else(|| PyRuntimeError::new_err("validated record is unavailable"))?;
            if let Some(line) = record
                .line_record()
                .map_err(|error| python_error(py, error))?
            {
                rows.push((
                    line.record_index,
                    line.payload_offset,
                    line.tag_raw,
                    PyBytes::new(py, &line.header_bytes).unbind(),
                    line.start,
                    line.end,
                    line.elevation,
                    line.thickness,
                ));
            }
        }
        Ok(rows)
    }

    fn line_records_3d(
        &self,
        py: Python<'_>,
        start: u64,
        count: u64,
    ) -> PyResult<Vec<super::line3d_scanning::LineRecord3DRow>> {
        let window = self.window(py, start, count)?;
        // Eager admission also rejects an unsupported profile for an empty page.
        let _admitted = self
            .inner
            .line_records_3d()
            .map_err(|error| python_error(py, error))?;
        let mut rows = Vec::new();
        for index in window {
            if let Some(line) = self
                .inner
                .line_record_3d(index)
                .map_err(|error| python_error(py, error))?
            {
                rows.push(super::line3d_scanning::row(py, line));
            }
        }
        Ok(rows)
    }

    fn package_references(
        &self,
        py: Python<'_>,
        start: u64,
        count: u64,
    ) -> PyResult<Vec<PackageReferenceRow>> {
        let mut rows = Vec::new();
        for index in self.window(py, start, count)? {
            if let Some(reference) = self
                .inner
                .package_reference(index)
                .map_err(|error| python_error(py, error))?
            {
                rows.push((
                    reference.record_index,
                    reference.payload_offset,
                    reference.continuation_index,
                    reference.continuation_offset,
                    PyBytes::new(py, &reference.header_bytes).unbind(),
                    PyBytes::new(py, &reference.name_field_bytes).unbind(),
                    PyBytes::new(py, reference.name_key()).unbind(),
                    reference.anchor_raw,
                    PyBytes::new(py, &reference.continuation_header_bytes).unbind(),
                    reference.placement_raw,
                ));
            }
        }
        Ok(rows)
    }

    #[staticmethod]
    fn apply_package_xy(
        py: Python<'_>,
        anchor_raw: [f64; 2],
        placement_raw: [f64; 6],
        point: [f64; 2],
    ) -> PyResult<[f64; 2]> {
        ezdracad_core::PackageReference2D::apply_xy_fields(anchor_raw, placement_raw, point)
            .map_err(|error| python_error(py, error))
    }
}

#[pyfunction]
pub(crate) fn scan_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    values: LimitValues,
) -> PyResult<RawHandle> {
    let limits = limits(values);
    limits
        .check_file_size(data.as_bytes().len() as u64)
        .map_err(|e| python_error(py, e))?;
    let owned = data.as_bytes().to_vec();
    let inner = py
        .detach(move || ezdracad_core::scan_records(&owned, &limits))
        .map_err(|e| python_error(py, e))?;
    Ok(RawHandle { inner })
}

#[pyfunction]
pub(crate) fn scan_file(py: Python<'_>, path: PathBuf, values: LimitValues) -> PyResult<RawHandle> {
    let limits = limits(values);
    let inner = py
        .detach(move || ezdracad_core::scan_file(&path, &limits))
        .map_err(|e| python_error(py, e))?;
    Ok(RawHandle { inner })
}
