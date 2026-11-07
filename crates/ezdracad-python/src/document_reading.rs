//! Document ownership and conversion only; all reading/qualification is in Rust.
use super::{limits, python_error, LimitValues};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::path::PathBuf;

pub(crate) type ReadValues = (
    String,
    String,
    String,
    LimitValues,
    (u64, u64, u64),
    (u64, u64, u64, u64, u64),
    Option<String>,
    (u64, u64),
);

#[pyfunction]
pub(crate) fn text_limit_defaults() -> (u64, u64) {
    let value = ezdracad_core::TextLimits::default();
    (value.max_encoded_bytes, value.max_output_bytes)
}

#[pyfunction]
pub(crate) fn read_limit_defaults() -> (u64, u64, u64, u64, u64) {
    let value = ezdracad_core::ReadLimits::default();
    (
        value.max_objects,
        value.max_entities,
        value.max_nodes,
        value.max_provenance_bytes,
        value.max_diagnostics,
    )
}
fn options(values: ReadValues) -> ezdracad_core::Result<ezdracad_core::ReadOptions> {
    Ok(ezdracad_core::ReadOptions {
        policy: ezdracad_core::ReadPolicy::from_id(&values.0)?,
        scope: ezdracad_core::ReadScope::from_id(&values.1)?,
        line_profile: ezdracad_core::Line2DProfile::from_id(&values.2)?,
        basic_profile: values
            .6
            .as_deref()
            .map(ezdracad_core::Basic2DProfile::from_id)
            .transpose()?,
        text_limits: ezdracad_core::TextLimits {
            max_encoded_bytes: values.7 .0,
            max_output_bytes: values.7 .1,
        },
        parse_limits: limits(values.3),
        scene_limits: ezdracad_core::SceneLimits {
            max_depth: values.4 .0,
            max_nodes: values.4 .1,
            max_lines: values.4 .2,
        },
        read_limits: ezdracad_core::ReadLimits {
            max_objects: values.5 .0,
            max_entities: values.5 .1,
            max_nodes: values.5 .2,
            max_provenance_bytes: values.5 .3,
            max_diagnostics: values.5 .4,
        },
    })
}
fn source_row<'py>(
    py: Python<'py>,
    source: &ezdracad_core::SourceRef,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("stream_path", &source.stream_path)?;
    row.set_item("offset_domain", source.offset_domain())?;
    row.set_item("table_domain", source.table_domain.as_str())?;
    row.set_item("definition_index", source.definition_index)?;
    row.set_item("record_index", source.record_index)?;
    row.set_item("payload_offset", source.payload_offset)?;
    row.set_item("byte_length", source.byte_length)?;
    Ok(row)
}
fn report_row<'py>(
    py: Python<'py>,
    report: &ezdracad_core::ReadReport,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("schema_version", 1)?;
    row.set_item("status", report.status.as_str())?;
    row.set_item("geometry_status", report.geometry_status.as_str())?;
    row.set_item("document_status", report.document_status.as_str())?;
    row.set_item("document_complete", report.document_complete())?;
    row.set_item("policy", report.policy.as_str())?;
    row.set_item("scope", report.scope.as_str())?;
    row.set_item("line_profile", report.line_profile.as_str())?;
    row.set_item("basic_profile", report.basic_profile.map(|p| p.as_str()))?;
    row.set_item("requested_entity_types", report.requested_entity_types())?;
    row.set_item("coordinate_semantics", report.coordinate_semantics())?;
    row.set_item("units", report.units())?;
    row.set_item(
        "unresolved_semantics",
        report
            .unresolved_semantics()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    )?;
    row.set_item("contents_sha256", &report.contents_sha256)?;
    row.set_item("source_format", report.source_format.as_str())?;
    row.set_item("stream_path", &report.stream_path)?;
    row.set_item("offset_domain", report.offset_domain())?;
    row.set_item("raw_record_count", report.raw_record_count)?;
    row.set_item("primary_object_count", report.primary_object_count)?;
    row.set_item("interior_record_count", report.interior_record_count)?;
    row.set_item("auxiliary_record_count", report.auxiliary_record_count)?;
    row.set_item(
        "handled_auxiliary_object_count",
        report.handled_auxiliary_object_count,
    )?;
    row.set_item("decoded_object_count", report.decoded_object_count)?;
    row.set_item("entity_count", report.entity_count)?;
    row.set_item("unsupported_counts", &report.unsupported_counts)?;
    row.set_item("context_blocked_counts", &report.context_blocked_counts)?;
    row.set_item("diagnostics_suppressed", report.diagnostics_suppressed)?;
    row.set_item(
        "definition_structure_validated",
        report.definition_structure_validated,
    )?;
    row.set_item("scene_nodes_visited", report.scene_nodes_visited)?;
    let diagnostics = report
        .diagnostics
        .iter()
        .map(|d| {
            let r = PyDict::new(py);
            r.set_item("code", d.code)?;
            r.set_item("message", &d.message)?;
            r.set_item(
                "source",
                d.source.as_ref().map(|s| source_row(py, s)).transpose()?,
            )?;
            Ok(r.unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    row.set_item("diagnostics", diagnostics)?;
    let regions = report
        .opaque_regions
        .iter()
        .map(|region| {
            let r = PyDict::new(py);
            r.set_item("stream_path", &region.stream_path)?;
            r.set_item("payload_offset", region.payload_offset)?;
            r.set_item("byte_length", region.byte_length)?;
            r.set_item("offset_domain", region.offset_domain())?;
            Ok(r.unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    row.set_item("opaque_regions", regions)?;
    Ok(row)
}
fn primitive_row<'py>(
    py: Python<'py>,
    bytes: &[u8],
    style: ezdracad_core::RootPrimitiveStyle,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item(
        "qualification_profile",
        ezdracad_core::Basic2DProfile::Marker25RootPrimitivesV1.as_str(),
    )?;
    row.set_item("coordinate_scope", "root_records")?;
    row.set_item("units", py.None())?;
    row.set_item("raw_bytes", PyBytes::new(py, bytes))?;
    row.set_item(
        "attributes",
        super::record_attribute_scanning::attribute_row(py, style.attributes),
    )?;
    row.set_item("style_layer_index", style.style_layer_index)?;
    row.set_item(
        "styles",
        super::effective_style_scanning::styles_row(py, style.styles)?,
    )?;
    Ok(row)
}
fn read_error(py: Python<'_>, error: ezdracad_core::ReadError) -> PyErr {
    let (cause, report) = error.into_parts();
    let error = python_error(py, cause);
    if let Some(report) = report {
        match report_row(py, &report).and_then(|row| error.value(py).setattr("_read_report", row)) {
            Ok(()) => {}
            Err(mapping_error) => return mapping_error,
        }
    }
    error
}
#[pyclass(name = "_DocumentHandle", frozen, module = "ezdracad._core")]
pub(crate) struct DocumentHandle {
    inner: ezdracad_core::Document,
}
#[pymethods]
impl DocumentHandle {
    fn report<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        report_row(py, self.inner.report())
    }
    fn query_count(&self, py: Python<'_>, entity_type: &str) -> PyResult<u64> {
        self.inner
            .query_count(entity_type)
            .map_err(|e| python_error(py, e))
    }
    fn query_page(
        &self,
        py: Python<'_>,
        entity_type: &str,
        start: u64,
        count: u64,
    ) -> PyResult<Vec<Py<PyDict>>> {
        let entities = py
            .detach(|| {
                self.inner
                    .query(entity_type, start, count)
                    .map(|values| values.into_iter().cloned().collect::<Vec<_>>())
            })
            .map_err(|e| python_error(py, e))?;
        let mut rows = Vec::with_capacity(entities.len());
        for entity in entities {
            let id = entity.entity_id();
            let root = entity.root_record_index();
            let source = entity.source_ref().clone();
            let kind = entity.entity_type();
            let row = match entity {
                ezdracad_core::Entity::Line(e) => {
                    let mut rows =
                        super::scene_scanning::rows(py, vec![e.geometry.scene().clone()])?;
                    let row = rows.pop().expect("one line row");
                    row.bind(py)
                        .set_item("qualification_profile", e.geometry.profile().as_str())?;
                    row
                }
                ezdracad_core::Entity::Circle(e) => {
                    let row = primitive_row(py, &e.geometry.raw_bytes, e.style)?;
                    row.set_item("center", e.geometry.center)?;
                    row.set_item("radius", e.geometry.radius)?;
                    row.unbind()
                }
                ezdracad_core::Entity::Arc(e) => {
                    let row = primitive_row(py, &e.geometry.raw_bytes, e.style)?;
                    row.set_item("center", e.geometry.center)?;
                    row.set_item("radius", e.geometry.radius)?;
                    row.set_item("start_angle_degrees", e.geometry.start_angle_degrees)?;
                    row.set_item("end_angle_degrees", e.geometry.end_angle_degrees)?;
                    row.unbind()
                }
                ezdracad_core::Entity::Text(e) => {
                    let v = e.geometry;
                    let row = primitive_row(py, &v.raw_bytes, e.style)?;
                    row.set_item("basepoint", v.basepoint)?;
                    row.set_item("width", v.width)?;
                    row.set_item("height", v.height)?;
                    row.set_item("spacing", v.spacing)?;
                    row.set_item("rotation_degrees", v.rotation_degrees)?;
                    row.set_item("origin", v.origin)?;
                    row.set_item("body_encoding", v.body_encoding())?;
                    row.set_item("font_name", v.font_name)?;
                    row.set_item("font_bytes", PyBytes::new(py, &v.font_bytes))?;
                    row.set_item("serialized_text", v.serialized_text)?;
                    row.set_item("body_bytes", PyBytes::new(py, &v.body_bytes))?;
                    row.set_item("text", v.text)?;
                    row.set_item("text_flags", v.text_flags)?;
                    row.set_item("charset", v.charset)?;
                    row.set_item("family", v.family)?;
                    row.set_item("scale_flag", v.scale_flag)?;
                    row.unbind()
                }
            };
            row.bind(py).set_item("type", kind)?;
            row.bind(py).set_item("entity_id", id)?;
            row.bind(py).set_item("root_record_index", root)?;
            row.bind(py)
                .set_item("source_ref", source_row(py, &source)?)?;
            rows.push(row);
        }
        Ok(rows)
    }
    fn raw(&self) -> super::scanning::RawHandle {
        super::scanning::RawHandle {
            inner: self.inner.raw().clone(),
        }
    }
}
#[pyfunction]
pub(crate) fn read_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    values: ReadValues,
) -> PyResult<DocumentHandle> {
    let options = options(values).map_err(|e| python_error(py, e))?;
    options
        .parse_limits
        .check_file_size(data.as_bytes().len() as u64)
        .map_err(|e| python_error(py, e))?;
    let owned = data.as_bytes().to_vec();
    let inner = py
        .detach(move || ezdracad_core::read(&owned, &options))
        .map_err(|e| read_error(py, e))?;
    Ok(DocumentHandle { inner })
}
#[pyfunction]
pub(crate) fn read_file(
    py: Python<'_>,
    path: PathBuf,
    values: ReadValues,
) -> PyResult<DocumentHandle> {
    let options = options(values).map_err(|e| python_error(py, e))?;
    let inner = py
        .detach(move || ezdracad_core::read_file(&path, &options))
        .map_err(|e| read_error(py, e))?;
    Ok(DocumentHandle { inner })
}
