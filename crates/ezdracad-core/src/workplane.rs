//! Explicit saved 3D workplane point maps in stored coordinate units.
//!
//! Preset axes come from the admitted native direction selector. Saved axis
//! bytes are used only for direction 0; under presets they may contain cached
//! state. These maps do not declare millimeters or creation scales.

use crate::preflight::invalid;
use crate::{
    coordinate_space_context, CoordinateSpaceEvidence, CoordinateSpaceProfile, DracadError,
    ObservedProfile, RawDocument, Result,
};

const HEADER_START: usize = 0x450;
const HEADER_END: usize = 0x4d0;
const AXIS_TOLERANCE: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkplaneProfile {
    Marker25SavedWorkplaneV1,
}

impl WorkplaneProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "marker25_saved_workplane_v1"
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker25_saved_workplane_v1" => Ok(Self::Marker25SavedWorkplaneV1),
            _ => Err(unsupported("unknown workplane profile")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkplaneDirection {
    Free,
    Plan,
    East,
    South,
    West,
    North,
}

impl WorkplaneDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Plan => "plan",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
            Self::North => "north",
        }
    }

    fn decode(raw: i32) -> Result<Self> {
        match raw {
            0 => Ok(Self::Free),
            1 => Ok(Self::Plan),
            2 => Ok(Self::East),
            3 => Ok(Self::South),
            4 => Ok(Self::West),
            5 => Ok(Self::North),
            _ => Err(unsupported("unknown saved workplane direction")),
        }
    }

    fn preset_axes(self) -> Option<[[f64; 3]; 3]> {
        match self {
            Self::Free => None,
            Self::Plan => Some([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
            Self::East => Some([[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]]),
            Self::South => Some([[1., 0., 0.], [0., 0., 1.], [0., -1., 0.]]),
            Self::West => Some([[0., -1., 0.], [0., 0., 1.], [-1., 0., 0.]]),
            Self::North => Some([[-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkplaneBasisSource {
    StoredAxesForFree,
    DerivedPreset,
}

impl WorkplaneBasisSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StoredAxesForFree => "stored_axes_for_free",
            Self::DerivedPreset => "derived_preset",
        }
    }
}

/// Exact Contents ranges retained independently of interpreted workplane fields.
///
/// The 128-byte plane header includes opaque pitch and selector-neighbor bytes.
/// Reading or preserving these bytes does not interpret them. The coordinate
/// context header authenticates the conservative unit/Draft/scale guards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkplaneEvidence {
    pub contents_sha256: String,
    pub observed_profile: ObservedProfile,
    pub header_offset: u64,
    pub header_bytes: [u8; 128],
    pub coordinate_guard: CoordinateSpaceEvidence,
}

/// Private construction binds the admitted stored point map to one snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkplaneContext {
    profile: WorkplaneProfile,
    direction: WorkplaneDirection,
    direction_raw: i32,
    origin: [f64; 3],
    stored_axes: [[f64; 3]; 3],
    effective_axes: [[f64; 3]; 3],
    basis_source: WorkplaneBasisSource,
    evidence: WorkplaneEvidence,
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn finite(value: f64, label: &str) -> Result<f64> {
    if !value.is_finite() {
        return Err(invalid(format!("non-finite workplane {label}")));
    }
    Ok(value)
}

fn product(a: f64, b: f64) -> Result<f64> {
    let value = finite(a * b, "matrix product")?;
    if a != 0. && b != 0. && value == 0. {
        return Err(invalid("workplane nonzero matrix product underflow"));
    }
    Ok(value)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> Result<f64> {
    finite(
        finite(product(a[0], b[0])? + product(a[1], b[1])?, "matrix sum")? + product(a[2], b[2])?,
        "matrix sum",
    )
}

fn validate_axes(axes: [[f64; 3]; 3]) -> Result<()> {
    for i in 0..3 {
        for j in i..3 {
            let expected = if i == j { 1. } else { 0. };
            if (dot(axes[i], axes[j])? - expected).abs() > AXIS_TOLERANCE {
                return Err(unsupported("saved free workplane axes are not orthonormal"));
            }
        }
    }
    let cross = [
        axes[0][1] * axes[1][2] - axes[0][2] * axes[1][1],
        axes[0][2] * axes[1][0] - axes[0][0] * axes[1][2],
        axes[0][0] * axes[1][1] - axes[0][1] * axes[1][0],
    ];
    if (dot(cross, axes[2])? - 1.).abs() > AXIS_TOLERANCE {
        return Err(unsupported(
            "saved free workplane axes are not right-handed",
        ));
    }
    Ok(())
}

impl WorkplaneContext {
    #[must_use]
    pub const fn profile(&self) -> WorkplaneProfile {
        self.profile
    }

    #[must_use]
    pub const fn direction(&self) -> WorkplaneDirection {
        self.direction
    }

    #[must_use]
    pub const fn direction_raw(&self) -> i32 {
        self.direction_raw
    }

    #[must_use]
    pub const fn origin(&self) -> [f64; 3] {
        self.origin
    }

    /// Saved axis vectors, not a record of previous free-mode orientation.
    #[must_use]
    pub const fn stored_axes(&self) -> [[f64; 3]; 3] {
        self.stored_axes
    }

    /// Workplane +X,+Y,+Z vectors expressed in stored world coordinates.
    #[must_use]
    pub const fn effective_axes(&self) -> [[f64; 3]; 3] {
        self.effective_axes
    }

    #[must_use]
    pub const fn basis_source(&self) -> WorkplaneBasisSource {
        self.basis_source
    }

    #[must_use]
    pub const fn evidence(&self) -> &WorkplaneEvidence {
        &self.evidence
    }

    /// No physical-unit declaration is made for either stored coordinate frame.
    #[must_use]
    pub const fn units(&self) -> Option<&'static str> {
        None
    }

    /// Project a supplied stored-world point onto the admitted saved workplane.
    /// A bare supplied point carries no entity identity or native authorship.
    pub fn from_world_stored_xyz(&self, point: [f64; 3]) -> Result<[f64; 3]> {
        let mut relative = [0.; 3];
        for i in 0..3 {
            finite(point[i], "input point")?;
            relative[i] = finite(point[i] - self.origin[i], "origin subtraction")?;
        }
        let mut result = [0.; 3];
        for (i, axis) in self.effective_axes.iter().enumerate() {
            result[i] = dot(relative, *axis)?;
        }
        if relative != [0.; 3] && result == [0.; 3] {
            return Err(invalid(
                "workplane nonzero transformed displacement underflow",
            ));
        }
        Ok(result)
    }

    /// Return a point in stored world axes; no source file is written/reopened.
    /// f64 inverses are numerical, not a promise of bitwise roundtrips.
    pub fn to_world_stored_xyz(&self, point: [f64; 3]) -> Result<[f64; 3]> {
        for value in point {
            finite(value, "input point")?;
        }
        let mut result = [0.; 3];
        for (i, value) in result.iter_mut().enumerate() {
            let column = [
                self.effective_axes[0][i],
                self.effective_axes[1][i],
                self.effective_axes[2][i],
            ];
            let displacement = dot(point, column)?;
            *value = finite(self.origin[i] + displacement, "origin addition")?;
            if displacement != 0. && *value == self.origin[i] {
                return Err(invalid(
                    "workplane nonzero displacement lost in origin addition",
                ));
            }
        }
        Ok(result)
    }
}

/// Read a conservative marker-25 saved workplane from an owned raw snapshot.
///
/// Unit code 0, zero-angle disabled object-0 Draft, and finite positive current
/// scales are admission limits. They do not convert these stored axes to mm.
/// General 3D references, views, camera/projection and creation scales are not
/// decoded by this point-context reader.
pub fn workplane_context(
    document: &RawDocument,
    profile: WorkplaneProfile,
) -> Result<WorkplaneContext> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported("workplane profile requires observed marker 25"));
    }
    let guard = coordinate_space_context(document, CoordinateSpaceProfile::Marker25StoredAxesV1)?;
    if guard.origin_units_raw() != 0
        || guard.draft().enable_raw != 0
        || guard.draft().object_raw != 0
        || guard.draft().angle_degrees != 0.
    {
        return Err(unsupported(
            "workplane profile requires input unit 0 and zero-angle disabled object-0 Draft",
        ));
    }
    let header_bytes: [u8; 128] = document
        .contents()
        .get(HEADER_START..HEADER_END)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| invalid("truncated saved workplane header"))?;
    let mut origin = [0.; 3];
    for (i, value) in origin.iter_mut().enumerate() {
        *value = finite(
            f64::from_le_bytes(header_bytes[i * 8..i * 8 + 8].try_into().unwrap()),
            "origin",
        )?;
    }
    let direction_raw = i32::from_le_bytes(header_bytes[0x30..0x34].try_into().unwrap());
    let direction = WorkplaneDirection::decode(direction_raw)?;
    let mut stored_axes = [[0.; 3]; 3];
    for (i, axis) in stored_axes.iter_mut().enumerate() {
        for (j, value) in axis.iter_mut().enumerate() {
            let at = 0x38 + (i * 3 + j) * 8;
            *value = finite(
                f64::from_le_bytes(header_bytes[at..at + 8].try_into().unwrap()),
                "stored axis",
            )?;
        }
    }
    let (effective_axes, basis_source) = match direction.preset_axes() {
        Some(axes) => (axes, WorkplaneBasisSource::DerivedPreset),
        None => {
            validate_axes(stored_axes)?;
            (stored_axes, WorkplaneBasisSource::StoredAxesForFree)
        }
    };
    Ok(WorkplaneContext {
        profile,
        direction,
        direction_raw,
        origin,
        stored_axes,
        effective_axes,
        basis_source,
        evidence: WorkplaneEvidence {
            contents_sha256: document.sha256().into(),
            observed_profile: document.header().profile,
            header_offset: HEADER_START as u64,
            header_bytes,
            coordinate_guard: guard.evidence().clone(),
        },
    })
}

impl RawDocument {
    pub fn workplane_context(&self, profile: WorkplaneProfile) -> Result<WorkplaneContext> {
        workplane_context(self, profile)
    }
}
