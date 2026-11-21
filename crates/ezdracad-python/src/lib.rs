//! Thin Python boundary; all container parsing is implemented by ezdracad-core.
#![forbid(unsafe_code)]

use ezdracad_core::{CfbEntryKind, ParseLimits};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

mod best_effort_reading;
mod block3d_scanning;
mod block_scanning;
mod coordinate_input_units_scanning;
mod coordinate_measurement_scanning;
mod coordinate_space_scanning;
mod dimension_scale_scanning;
mod document_reading;
mod effective_style_scanning;
mod inline_scanning;
mod inspection;
mod line3d_scanning;
mod origin_units_scanning;
mod print_settings_scanning;
mod print_width_scanning;
mod qualification_scanning;
mod record_attribute_scanning;
mod scanning;
mod scene_scanning;
mod style_scanning;
mod unit_resolution_scanning;
mod workplane_scanning;
mod world_input3d_scanning;

pub(crate) type LimitValues = (u64, u64, u64, u64, u64, u64, u64, u64, u64);

pub(crate) fn limits(values: LimitValues) -> ParseLimits {
    ParseLimits {
        max_file_size: values.0,
        max_cfb_entries: values.1,
        max_stream_size: values.2,
        max_total_stream_size: values.3,
        max_cfb_metadata_size: values.4,
        max_cfb_directory_slots: values.5,
        max_cfb_depth: values.6,
        max_records: values.7,
        max_decompressed_size: values.8,
    }
}

pub(crate) fn cfb_row<'py>(
    py: Python<'py>,
    info: &ezdracad_core::CfbInfo,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("version", info.version)?;
    row.set_item("root_clsid", &info.root_clsid)?;
    row.set_item("total_stream_size", info.total_stream_size)?;
    row.set_item("metadata_size", info.metadata_size)?;
    row.set_item("directory_slots", info.directory_slots)?;
    row.set_item("depth", info.depth)?;
    let entries: Vec<_> = info
        .entries
        .iter()
        .map(|entry| {
            let kind = match entry.kind {
                CfbEntryKind::Storage => "storage",
                CfbEntryKind::Stream => "stream",
            };
            (entry.path.as_str(), kind, entry.size)
        })
        .collect();
    row.set_item("entries", entries)?;
    Ok(row)
}

pub(crate) fn mps_row<'py>(
    py: Python<'py>,
    info: &ezdracad_core::MpsInfo,
) -> PyResult<Bound<'py, PyDict>> {
    let row = PyDict::new(py);
    row.set_item("stream_path", &info.stream_path)?;
    row.set_item("stream_size", info.stream_size)?;
    row.set_item("stream_sha256", &info.stream_sha256)?;
    row.set_item("wrapper_profile", info.wrapper_profile)?;
    row.set_item("compression", info.compression.as_str())?;
    row.set_item("decoded_size", info.decoded_size)?;
    let chunks: Vec<_> = info
        .chunks
        .iter()
        .map(|chunk| -> PyResult<_> {
            let value = PyDict::new(py);
            value.set_item("encoded_offset", chunk.encoded_offset)?;
            value.set_item("encoded_size", chunk.encoded_size)?;
            value.set_item("decoded_offset", chunk.decoded_offset)?;
            value.set_item("decoded_size", chunk.decoded_size)?;
            Ok(value)
        })
        .collect::<PyResult<_>>()?;
    row.set_item("chunks", chunks)?;
    Ok(row)
}

create_exception!(ezdracad, DracadError, PyException, "Base ezdracad error.");
create_exception!(
    ezdracad,
    InvalidFormatError,
    DracadError,
    "Malformed container or data."
);
create_exception!(
    ezdracad,
    UnsupportedFormatError,
    DracadError,
    "Recognized but unsupported format."
);
create_exception!(
    ezdracad,
    UnsupportedFeatureError,
    DracadError,
    "Recognized but unsupported feature."
);
create_exception!(
    ezdracad,
    LimitExceededError,
    DracadError,
    "A configured resource limit was exceeded."
);
create_exception!(
    ezdracad,
    DracadIoError,
    DracadError,
    "Input/output failure."
);

fn python_error(py: Python<'_>, error: ezdracad_core::DracadError) -> PyErr {
    match error {
        ezdracad_core::DracadError::InvalidFormat(message) => InvalidFormatError::new_err(message),
        ezdracad_core::DracadError::UnsupportedFormat(message) => {
            UnsupportedFormatError::new_err(message)
        }
        ezdracad_core::DracadError::UnsupportedFeature(message) => {
            UnsupportedFeatureError::new_err(message)
        }
        ezdracad_core::DracadError::LimitExceeded {
            resource,
            limit,
            actual,
        } => {
            let error = LimitExceededError::new_err(format!(
                "{resource} limit exceeded: {actual} > {limit}"
            ));
            let attributes = (|| {
                let value = error.value(py);
                value.setattr("resource", resource)?;
                value.setattr("limit", limit)?;
                value.setattr("actual", actual)?;
                Ok::<_, PyErr>(())
            })();
            match attributes {
                Ok(()) => error,
                Err(attribute_error) => attribute_error,
            }
        }
        ezdracad_core::DracadError::Io(error) => DracadIoError::new_err(error.to_string()),
        other => DracadError::new_err(other.to_string()),
    }
}

#[pyfunction]
fn core_version() -> &'static str {
    ezdracad_core::version()
}

#[pyfunction]
fn inspect_cfb_bytes<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    values: LimitValues,
) -> PyResult<Bound<'py, PyDict>> {
    let limits = limits(values);
    limits
        .check_file_size(data.as_bytes().len() as u64)
        .map_err(|error| python_error(py, error))?;
    // The detached closure owns its bytes and holds no Python references.
    let owned = data.as_bytes().to_vec();
    let info = py
        .detach(move || ezdracad_core::inspect_cfb(&owned, &limits))
        .map_err(|error| python_error(py, error))?;
    cfb_row(py, &info)
}

#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(core_version, module)?)?;
    module.add_function(wrap_pyfunction!(
        record_attribute_scanning::decode_record_attributes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(inspect_cfb_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(inspection::inspect_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(inspection::inspect_file, module)?)?;
    module.add_function(wrap_pyfunction!(document_reading::read_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(
        document_reading::text_limit_defaults,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        document_reading::read_limit_defaults,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(document_reading::read_file, module)?)?;
    module.add_function(wrap_pyfunction!(
        best_effort_reading::best_effort_limit_defaults,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        best_effort_reading::read_best_effort_bytes,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        best_effort_reading::read_best_effort_file,
        module
    )?)?;
    module.add_class::<document_reading::DocumentHandle>()?;
    module.add_class::<inspection::ContainerHandle>()?;
    module.add_function(wrap_pyfunction!(scanning::scan_bytes, module)?)?;
    module.add_function(wrap_pyfunction!(scanning::scan_file, module)?)?;
    module.add_function(wrap_pyfunction!(
        print_width_scanning::resolve_nominal_print_width,
        module
    )?)?;
    module.add_class::<scanning::RawHandle>()?;
    module.add_class::<block3d_scanning::BlockReference3DHandle>()?;
    module.add_class::<block_scanning::BlockHandle>()?;
    module.add_class::<style_scanning::StyleHandle>()?;
    module.add_class::<coordinate_measurement_scanning::CoordinateMeasurementHandle>()?;
    module.add_class::<coordinate_space_scanning::CoordinateSpaceHandle>()?;
    module.add_class::<coordinate_input_units_scanning::NativeCoordinateInputUnitHandle>()?;
    module.add_class::<print_settings_scanning::PrintSettingsHandle>()?;
    module.add_class::<unit_resolution_scanning::UnitResolutionHandle>()?;
    module.add_class::<unit_resolution_scanning::DeclaredCreationScaleHandle>()?;
    module.add_class::<workplane_scanning::WorkplaneHandle>()?;
    module.add_class::<world_input3d_scanning::WorldInput3DHandle>()?;
    let py = module.py();
    module.add("DracadError", py.get_type::<DracadError>())?;
    module.add("InvalidFormatError", py.get_type::<InvalidFormatError>())?;
    module.add(
        "UnsupportedFormatError",
        py.get_type::<UnsupportedFormatError>(),
    )?;
    module.add(
        "UnsupportedFeatureError",
        py.get_type::<UnsupportedFeatureError>(),
    )?;
    module.add("LimitExceededError", py.get_type::<LimitExceededError>())?;
    module.add("DracadIoError", py.get_type::<DracadIoError>())?;
    module.add(
        "DEFAULT_MAX_FILE_SIZE",
        ezdracad_core::DEFAULT_MAX_FILE_SIZE,
    )?;
    module.add(
        "DEFAULT_MAX_CFB_ENTRIES",
        ezdracad_core::DEFAULT_MAX_CFB_ENTRIES,
    )?;
    module.add(
        "DEFAULT_MAX_STREAM_SIZE",
        ezdracad_core::DEFAULT_MAX_STREAM_SIZE,
    )?;
    module.add(
        "DEFAULT_MAX_TOTAL_STREAM_SIZE",
        ezdracad_core::DEFAULT_MAX_TOTAL_STREAM_SIZE,
    )?;
    module.add(
        "DEFAULT_MAX_CFB_METADATA_SIZE",
        ezdracad_core::DEFAULT_MAX_CFB_METADATA_SIZE,
    )?;
    module.add(
        "DEFAULT_MAX_CFB_DIRECTORY_SLOTS",
        ezdracad_core::DEFAULT_MAX_CFB_DIRECTORY_SLOTS,
    )?;
    module.add(
        "DEFAULT_MAX_CFB_DEPTH",
        ezdracad_core::DEFAULT_MAX_CFB_DEPTH,
    )?;
    module.add("DEFAULT_MAX_RECORDS", ezdracad_core::DEFAULT_MAX_RECORDS)?;
    module.add(
        "DEFAULT_MAX_DECOMPRESSED_SIZE",
        ezdracad_core::DEFAULT_MAX_DECOMPRESSED_SIZE,
    )?;
    module.add("MAX_RECORDS_PER_PAGE", ezdracad_core::MAX_RECORDS_PER_PAGE)?;
    module.add(
        "MAX_INLINE_PACKAGE_DEPTH",
        ezdracad_core::MAX_INLINE_PACKAGE_DEPTH,
    )?;
    module.add(
        "MAX_INLINE_PACKAGE_PROVENANCE",
        ezdracad_core::MAX_INLINE_PACKAGE_PROVENANCE,
    )?;
    Ok(())
}
