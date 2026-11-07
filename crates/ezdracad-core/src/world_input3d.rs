//! Explicit loaded-file 3D absolute-input convention, not entity physical units.

use crate::preflight::invalid;
use crate::{
    coordinate_space_context, CoordinateFrame, CoordinateSpaceContext, CoordinateSpaceEvidence,
    CoordinateSpaceProfile, DracadError, NativeCoordinateInputUnitInfo, ObservedProfile,
    RawDocument, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldInput3DProfile {
    Marker25LoadedWorldInputV1,
}

impl WorldInput3DProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "marker25_loaded_world_input_v1"
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker25_loaded_world_input_v1" => Ok(Self::Marker25LoadedWorldInputV1),
            _ => Err(unsupported("unknown loaded world-input 3D profile")),
        }
    }
}

/// Saved current settings under the independently tested loaded-file convention.
///
/// Supplied points must already be in stored world axes. This context does not
/// establish an arbitrary record's parent placement or physical units. The Z
/// input origin is the observed loaded-file zero convention, not a decoded
/// origin-Z field; +0x158 is not interpreted as world-origin Z.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldInput3DContext {
    profile: WorldInput3DProfile,
    coordinate_context: CoordinateSpaceContext,
    draw_origin_bytes: [u8; 4],
    draw_origin_raw: i32,
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn finite_point(point: [f64; 3]) -> Result<()> {
    if point.into_iter().any(|value| !value.is_finite()) {
        return Err(invalid("non-finite loaded world-input 3D point"));
    }
    Ok(())
}

fn scaled_z(source: f64, converted: f64) -> Result<f64> {
    if !converted.is_finite() {
        return Err(invalid("loaded world-input 3D Z scaling overflow"));
    }
    if source != 0. && converted == 0. {
        return Err(invalid("loaded world-input 3D nonzero Z scaling underflow"));
    }
    Ok(converted)
}

impl WorldInput3DContext {
    #[must_use]
    pub const fn profile(&self) -> WorldInput3DProfile {
        self.profile
    }

    #[must_use]
    pub const fn coordinate_context(&self) -> &CoordinateSpaceContext {
        &self.coordinate_context
    }

    #[must_use]
    pub const fn evidence(&self) -> &CoordinateSpaceEvidence {
        self.coordinate_context.evidence()
    }

    #[must_use]
    pub const fn input_unit(&self) -> &NativeCoordinateInputUnitInfo {
        self.coordinate_context.input_unit()
    }

    /// Saved XY plus the tested loaded-file zero-Z convention, not a 3D field.
    #[must_use]
    pub const fn loaded_world_origin(&self) -> [f64; 3] {
        let xy = self.coordinate_context.drawing_origin();
        [xy[0], xy[1], 0.]
    }

    #[must_use]
    pub const fn origin_z_source(&self) -> &'static str {
        "loaded_file_zero_z_convention"
    }

    #[must_use]
    pub const fn draw_origin_offset(&self) -> u64 {
        0x154
    }

    #[must_use]
    pub const fn draw_origin_bytes(&self) -> [u8; 4] {
        self.draw_origin_bytes
    }

    #[must_use]
    pub const fn draw_origin_raw(&self) -> i32 {
        self.draw_origin_raw
    }

    /// Units of this named input convention; source geometry units stay unknown.
    #[must_use]
    pub const fn units(&self) -> &'static str {
        "mm"
    }

    #[must_use]
    pub const fn coordinate_semantics(&self) -> &'static str {
        "loaded_world_absolute_input_mm"
    }

    /// q_mm = (stored_world - (saved_origin_xy, 0)) / current_ratio.
    pub fn input_mm_from_world_stored_xyz(&self, point: [f64; 3]) -> Result<[f64; 3]> {
        finite_point(point)?;
        let xy = self.coordinate_context.from_root_stored_xy(
            [point[0], point[1]],
            CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
        )?;
        let z = scaled_z(
            point[2],
            point[2] / self.coordinate_context.measurement().annotation_ratio,
        )?;
        Ok([xy[0], xy[1], z])
    }

    /// stored_world = (saved_origin_xy, 0) + current_ratio * q_mm.
    pub fn world_stored_xyz_from_input_mm(&self, point: [f64; 3]) -> Result<[f64; 3]> {
        finite_point(point)?;
        let xy = self.coordinate_context.to_root_stored_xy(
            [point[0], point[1]],
            CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
        )?;
        let z = scaled_z(
            point[2],
            point[2] * self.coordinate_context.measurement().annotation_ratio,
        )?;
        Ok([xy[0], xy[1], z])
    }
}

/// Read an explicit marker25 loaded-file 3D world absolute-input convention.
///
/// Native codes 0 and 2, DrawOrigin=0, disabled object-0 zero-angle Draft and the
/// existing finite positive current-scale guards are required. This excludes
/// transient live application state after origin editing; saved source bytes
/// cannot authenticate that live cache. No parent/scene/history is inferred.
pub fn world_input_3d_context(
    document: &RawDocument,
    profile: WorldInput3DProfile,
) -> Result<WorldInput3DContext> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported(
            "loaded world-input 3D profile requires marker25",
        ));
    }
    let coordinate_context =
        coordinate_space_context(document, CoordinateSpaceProfile::Marker25StoredAxesV1)?;
    let draw_origin_bytes: [u8; 4] = coordinate_context.evidence().header_bytes[20..24]
        .try_into()
        .unwrap();
    let draw_origin_raw = i32::from_le_bytes(draw_origin_bytes);
    let draft = coordinate_context.draft();
    if !matches!(coordinate_context.origin_units_raw(), 0 | 2)
        || draw_origin_raw != 0
        || draft.enable_raw != 0
        || draft.object_raw != 0
        || draft.angle_degrees != 0.
    {
        return Err(unsupported(
            "loaded world-input 3D profile requires native input code0/2, DrawOrigin0 and disabled object-0 zero-angle Draft",
        ));
    }
    Ok(WorldInput3DContext {
        profile,
        coordinate_context,
        draw_origin_bytes,
        draw_origin_raw,
    })
}

impl RawDocument {
    pub fn world_input_3d_context(
        &self,
        profile: WorldInput3DProfile,
    ) -> Result<WorldInput3DContext> {
        world_input_3d_context(self, profile)
    }
}
