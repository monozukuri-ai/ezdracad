//! Explicit point frames derived from a saved drawing context.
//!
//! These are named contextual projections of root stored axes. They do not
//! recover entity creation scales, decode a 3D work plane or resolve drawing WCS.

use crate::preflight::invalid;
use crate::{
    CoordinateMeasurementContext, DracadError, Line2DProfile, NativeCoordinateInputUnitInfo,
    ObservedProfile, OriginUnitsCode, QualifiedLineSelection, RawDocument, Result, SceneLimits,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSpaceProfile {
    Marker5StoredAxesV1,
    Marker8StoredAxesV1,
    Marker25StoredAxesV1,
}

impl CoordinateSpaceProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Marker5StoredAxesV1 => "marker5_stored_axes_v1",
            Self::Marker8StoredAxesV1 => "marker8_stored_axes_v1",
            Self::Marker25StoredAxesV1 => "marker25_stored_axes_v1",
        }
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker5_stored_axes_v1" => Ok(Self::Marker5StoredAxesV1),
            "marker8_stored_axes_v1" => Ok(Self::Marker8StoredAxesV1),
            "marker25_stored_axes_v1" => Ok(Self::Marker25StoredAxesV1),
            _ => Err(unsupported("unknown coordinate-space profile")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFrame {
    RootStoredXy,
    DrawingOriginRelativeStoredXy,
    CurrentAnnotationMmFromDrawingOrigin,
    CurrentAbsoluteInputMmFromDrawingOrigin,
    ConfiguredPaperMmOffsetFromDrawingOrigin,
}

impl CoordinateFrame {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RootStoredXy => "root_stored_xy",
            Self::DrawingOriginRelativeStoredXy => "drawing_origin_relative_stored_xy",
            Self::CurrentAnnotationMmFromDrawingOrigin => {
                "current_annotation_mm_from_drawing_origin"
            }
            Self::CurrentAbsoluteInputMmFromDrawingOrigin => {
                "current_absolute_input_mm_from_drawing_origin"
            }
            Self::ConfiguredPaperMmOffsetFromDrawingOrigin => {
                "configured_paper_mm_offset_from_drawing_origin"
            }
        }
    }

    #[must_use]
    pub const fn units(self) -> Option<&'static str> {
        match self {
            Self::RootStoredXy | Self::DrawingOriginRelativeStoredXy => None,
            Self::CurrentAnnotationMmFromDrawingOrigin
            | Self::CurrentAbsoluteInputMmFromDrawingOrigin
            | Self::ConfiguredPaperMmOffsetFromDrawingOrigin => Some("mm"),
        }
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "root_stored_xy" => Ok(Self::RootStoredXy),
            "drawing_origin_relative_stored_xy" => Ok(Self::DrawingOriginRelativeStoredXy),
            "current_annotation_mm_from_drawing_origin" => {
                Ok(Self::CurrentAnnotationMmFromDrawingOrigin)
            }
            "current_absolute_input_mm_from_drawing_origin" => {
                Ok(Self::CurrentAbsoluteInputMmFromDrawingOrigin)
            }
            "configured_paper_mm_offset_from_drawing_origin" => {
                Ok(Self::ConfiguredPaperMmOffsetFromDrawingOrigin)
            }
            _ => Err(unsupported("unknown coordinate frame")),
        }
    }
}

/// Exact source header retained beside the interpreted context fields.
///
/// SHA-256 identifies Contents, rather than the enclosing CFB file. Header bytes
/// cover Contents 0x140..0x280, including uninterpreted fields between the known
/// origin, units, scale and saved axes state. Retaining them does not decode them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateSpaceEvidence {
    pub contents_sha256: String,
    pub observed_profile: ObservedProfile,
    pub header_offset: u64,
    pub header_bytes: [u8; 320],
}

/// Source fields associated with native MPXL Draft. Selector codes stay raw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedDraftState {
    pub angle_offset: u64,
    pub angle_bytes: [u8; 8],
    pub angle_degrees: f64,
    pub object_offset: u64,
    pub object_bytes: [u8; 4],
    pub object_raw: i32,
    pub enable_offset: u64,
    pub enable_bytes: [u8; 4],
    pub enable_raw: i32,
}

/// Private construction keeps source decoding and profile checks on this path.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateSpaceContext {
    profile: CoordinateSpaceProfile,
    drawing_origin: [f64; 2],
    origin_units_raw: i32,
    input_unit: NativeCoordinateInputUnitInfo,
    draft: SavedDraftState,
    measurement: CoordinateMeasurementContext,
    evidence: CoordinateSpaceEvidence,
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn finite(value: f64, label: &str) -> Result<f64> {
    if !value.is_finite() {
        return Err(invalid(format!("non-finite coordinate-space {label}")));
    }
    Ok(value)
}

fn scaled(input: f64, output: f64) -> Result<f64> {
    finite(output, "scaled value")?;
    if input != 0.0 && output == 0.0 {
        return Err(invalid("coordinate-space nonzero displacement underflow"));
    }
    Ok(output)
}

fn rotated(point: [f64; 2], angle_degrees: f64) -> Result<[f64; 2]> {
    // Exact cardinal values avoid introducing a tiny orthogonal displacement
    // that a later origin translation could legitimately round away.
    let (sine, cosine) = match angle_degrees.rem_euclid(360.0) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => angle_degrees.to_radians().sin_cos(),
    };
    let result = [
        finite(
            finite(point[0] * cosine, "rotation term")? - finite(point[1] * sine, "rotation term")?,
            "rotated X",
        )?,
        finite(
            finite(point[0] * sine, "rotation term")? + finite(point[1] * cosine, "rotation term")?,
            "rotated Y",
        )?,
    ];
    if point != [0.0, 0.0] && result == [0.0, 0.0] {
        return Err(invalid("coordinate-space nonzero rotation underflow"));
    }
    Ok(result)
}

impl CoordinateSpaceContext {
    #[must_use]
    pub const fn profile(&self) -> CoordinateSpaceProfile {
        self.profile
    }

    #[must_use]
    pub const fn drawing_origin(&self) -> [f64; 2] {
        self.drawing_origin
    }

    #[must_use]
    pub const fn origin_units_raw(&self) -> i32 {
        self.origin_units_raw
    }

    #[must_use]
    pub const fn input_unit(&self) -> &NativeCoordinateInputUnitInfo {
        &self.input_unit
    }

    #[must_use]
    pub const fn draft(&self) -> &SavedDraftState {
        &self.draft
    }

    #[must_use]
    pub const fn measurement(&self) -> &CoordinateMeasurementContext {
        &self.measurement
    }

    #[must_use]
    pub const fn evidence(&self) -> &CoordinateSpaceEvidence {
        &self.evidence
    }

    /// Explicitly map a supplied root-stored point into a named saved context.
    ///
    /// A bare point carries no entity provenance. Call the selection function
    /// for points decoded and placed within the same owned document snapshot.
    pub fn from_root_stored_xy(&self, point: [f64; 2], frame: CoordinateFrame) -> Result<[f64; 2]> {
        let mut result = [0.0; 2];
        for i in 0..2 {
            finite(point[i], "input point")?;
            if frame == CoordinateFrame::RootStoredXy {
                result[i] = point[i];
                continue;
            }
            let relative = finite(point[i] - self.drawing_origin[i], "origin subtraction")?;
            result[i] = match frame {
                CoordinateFrame::RootStoredXy => unreachable!(),
                CoordinateFrame::DrawingOriginRelativeStoredXy => relative,
                CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin
                | CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin => {
                    scaled(relative, relative / self.measurement.annotation_ratio)?
                }
                CoordinateFrame::ConfiguredPaperMmOffsetFromDrawingOrigin => {
                    scaled(relative, relative * self.measurement.paper_scale_factor)?
                }
            };
        }
        if frame == CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin
            && self.draft.enable_raw == 1
        {
            result = rotated(result, -self.draft.angle_degrees)?;
        }
        Ok(result)
    }

    /// Invert the named point map; this does not write to the source document.
    ///
    /// Arithmetic is f64, with explicit nonfinite/underflow checks. Bitwise
    /// reversibility for every possible f64 input is not promised.
    pub fn to_root_stored_xy(&self, point: [f64; 2], frame: CoordinateFrame) -> Result<[f64; 2]> {
        for coordinate in point {
            finite(coordinate, "input point")?;
        }
        let point = if frame == CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin
            && self.draft.enable_raw == 1
        {
            rotated(point, self.draft.angle_degrees)?
        } else {
            point
        };
        let mut result = [0.0; 2];
        for i in 0..2 {
            finite(point[i], "input point")?;
            if frame == CoordinateFrame::RootStoredXy {
                result[i] = point[i];
                continue;
            }
            let relative = match frame {
                CoordinateFrame::RootStoredXy => unreachable!(),
                CoordinateFrame::DrawingOriginRelativeStoredXy => point[i],
                CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin
                | CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin => {
                    scaled(point[i], point[i] * self.measurement.annotation_ratio)?
                }
                CoordinateFrame::ConfiguredPaperMmOffsetFromDrawingOrigin => {
                    scaled(point[i], point[i] / self.measurement.paper_scale_factor)?
                }
            };
            result[i] = finite(relative + self.drawing_origin[i], "origin translation")?;
            if relative != 0.0 && result[i] == self.drawing_origin[i] {
                return Err(invalid(
                    "coordinate-space nonzero translation lost precision",
                ));
            }
        }
        Ok(result)
    }
}

/// Decode one explicitly admitted context. No per-entity unit is inferred.
pub fn coordinate_space_context(
    document: &RawDocument,
    profile: CoordinateSpaceProfile,
) -> Result<CoordinateSpaceContext> {
    let marker = match profile {
        CoordinateSpaceProfile::Marker5StoredAxesV1 => ObservedProfile::Marker5,
        CoordinateSpaceProfile::Marker8StoredAxesV1 => ObservedProfile::Marker8,
        CoordinateSpaceProfile::Marker25StoredAxesV1 => ObservedProfile::Marker25,
    };
    if document.header().profile != marker {
        return Err(unsupported(
            "coordinate-space profile differs from source marker",
        ));
    }
    let contents = document.contents();
    let header_bytes: [u8; 320] = contents
        .get(0x140..0x280)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| invalid("truncated coordinate-space source header"))?;
    let drawing_origin = document.drawing_settings()?.drawing_origin;
    let selector_bytes = header_bytes[16..20].try_into().unwrap();
    let origin_units_raw = i32::from_le_bytes(selector_bytes);
    let input_unit = NativeCoordinateInputUnitInfo::from_origin_units_code(OriginUnitsCode {
        payload_offset: 0x150,
        selector_bytes,
        raw_code: origin_units_raw,
    });
    if input_unit.native_display_factor().is_none()
        || (marker != ObservedProfile::Marker25 && origin_units_raw != 0)
    {
        return Err(unsupported(
            "coordinate-space profile requires known marker25 input unit or legacy raw unit 0",
        ));
    }
    let angle_bytes = header_bytes[0x70..0x78].try_into().unwrap();
    let object_bytes = header_bytes[0x6c..0x70].try_into().unwrap();
    let enable_bytes = header_bytes[0x134..0x138].try_into().unwrap();
    let draft = SavedDraftState {
        angle_offset: 0x1b0,
        angle_bytes,
        angle_degrees: finite(f64::from_le_bytes(angle_bytes), "saved draft angle")?,
        object_offset: 0x1ac,
        object_bytes,
        object_raw: i32::from_le_bytes(object_bytes),
        enable_offset: 0x274,
        enable_bytes,
        enable_raw: i32::from_le_bytes(enable_bytes),
    };
    if !(-360.0..=360.0).contains(&draft.angle_degrees)
        || !matches!(draft.object_raw, 0 | 1)
        || !matches!(draft.enable_raw, 0 | 1)
    {
        return Err(unsupported(
            "coordinate-space profile requires known Draft selectors and angle within [-360,360]",
        ));
    }
    if marker != ObservedProfile::Marker25
        && (draft.angle_degrees != 0.0 || draft.object_raw != 0 || draft.enable_raw != 0)
    {
        return Err(unsupported(
            "legacy coordinate-space profile requires zero-angle, object-0, disabled Draft",
        ));
    }
    let measurement = document.coordinate_measurement_context()?;
    Ok(CoordinateSpaceContext {
        profile,
        drawing_origin,
        origin_units_raw,
        input_unit,
        draft,
        measurement,
        evidence: CoordinateSpaceEvidence {
            contents_sha256: document.sha256().into(),
            observed_profile: document.header().profile,
            header_offset: 0x140,
            header_bytes,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramedLineEndpoints {
    pub start: [f64; 2],
    pub end: [f64; 2],
}

/// A complete selected bounded subtree in one named frame, never a Document.
/// Original qualified lines and all their evidence are retained once in source.
#[derive(Debug, Clone, PartialEq)]
pub struct FramedLineSelection {
    source: QualifiedLineSelection,
    context: CoordinateSpaceContext,
    frame: CoordinateFrame,
    coordinates: Vec<FramedLineEndpoints>,
}

impl FramedLineSelection {
    #[must_use]
    pub const fn source(&self) -> &QualifiedLineSelection {
        &self.source
    }

    #[must_use]
    pub const fn context(&self) -> &CoordinateSpaceContext {
        &self.context
    }

    #[must_use]
    pub const fn frame(&self) -> CoordinateFrame {
        self.frame
    }

    #[must_use]
    pub fn coordinates(&self) -> &[FramedLineEndpoints] {
        &self.coordinates
    }

    #[must_use]
    pub const fn document_complete(&self) -> bool {
        false
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        QualifiedLineSelection,
        CoordinateSpaceContext,
        CoordinateFrame,
        Vec<FramedLineEndpoints>,
    ) {
        (self.source, self.context, self.frame, self.coordinates)
    }
}

/// Qualify/place first, then map root stored points using the same document.
/// A failure returns no partial selection and changes no existing profile.
pub fn qualify_lines_in_coordinate_space(
    document: &RawDocument,
    index: u64,
    line_profile: Line2DProfile,
    space_profile: CoordinateSpaceProfile,
    frame: CoordinateFrame,
    limits: &SceneLimits,
) -> Result<FramedLineSelection> {
    let context = coordinate_space_context(document, space_profile)?;
    let source = document.qualified_lines(index, line_profile, limits)?;
    let coordinates = source
        .lines()
        .iter()
        .map(|line| {
            Ok(FramedLineEndpoints {
                start: context.from_root_stored_xy(line.scene().start, frame)?,
                end: context.from_root_stored_xy(line.scene().end, frame)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(FramedLineSelection {
        source,
        context,
        frame,
        coordinates,
    })
}
