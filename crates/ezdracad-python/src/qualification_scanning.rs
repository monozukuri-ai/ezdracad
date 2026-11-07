use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn selection_row(
    py: Python<'_>,
    value: ezdracad_core::QualifiedLineSelection,
) -> PyResult<Bound<'_, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("record_index", value.record_index())?;
    row.set_item("profile", value.profile().as_str())?;
    row.set_item("coordinate_semantics", value.coordinate_semantics())?;
    row.set_item("units", py.None())?;
    row.set_item("document_complete", value.document_complete())?;
    row.set_item(
        "unresolved_semantics",
        ezdracad_core::LINE2D_UNRESOLVED_SEMANTICS
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
    )?;
    let scenes = value
        .into_lines()
        .into_iter()
        .map(ezdracad_core::Line2D::into_scene)
        .collect();
    row.set_item("lines", super::scene_scanning::rows(py, scenes)?)?;
    Ok(row)
}
