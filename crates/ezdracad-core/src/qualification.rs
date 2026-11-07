//! Versioned qualification of a bounded stored-XY line subset.
//!
//! Qualification describes the decoder contract. It does not attest that an
//! input was authored by DRA-CAD, or certify a complete/visible drawing.

use crate::preflight::invalid;
use crate::{
    AttributeReferenceKind, DracadError, ObservedProfile, RawDocument, RecordAttributes, Result,
    SceneAncestor, SceneAncestorKind, SceneLimits, SceneLine2D,
};

pub const LINE2D_V1_MAX_REFERENCE_DEPTH: u64 = 2;
pub const LINE2D_V1_MAX_INLINE_DEPTH: u64 = 2;
pub const LINE2D_V1_MAX_ANCESTORS: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line2DProfile {
    Marker25PlanarStoredXyV1,
    Marker25PlanarStoredXyInlineV1,
}

impl Line2DProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Marker25PlanarStoredXyV1 => "marker25_planar_stored_xy_v1",
            Self::Marker25PlanarStoredXyInlineV1 => "marker25_planar_stored_xy_inline_v1",
        }
    }

    pub fn from_id(value: &str) -> Result<Self> {
        match value {
            "marker25_planar_stored_xy_v1" => Ok(Self::Marker25PlanarStoredXyV1),
            "marker25_planar_stored_xy_inline_v1" => Ok(Self::Marker25PlanarStoredXyInlineV1),
            _ => Err(unsupported("unknown Line2D qualification profile")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedLineSemantics {
    PhysicalUnits,
    DrawingWcs,
    Visibility,
    Layout,
    MaterialMeaning,
    GroupMeaning,
    FinalPrintedAppearance,
    DocumentCompleteness,
}

impl UnresolvedLineSemantics {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhysicalUnits => "physical_units",
            Self::DrawingWcs => "drawing_wcs",
            Self::Visibility => "visibility",
            Self::Layout => "layout",
            Self::MaterialMeaning => "material_meaning",
            Self::GroupMeaning => "group_meaning",
            Self::FinalPrintedAppearance => "final_printed_appearance",
            Self::DocumentCompleteness => "document_completeness",
        }
    }
}

pub const LINE2D_UNRESOLVED_SEMANTICS: [UnresolvedLineSemantics; 8] = [
    UnresolvedLineSemantics::PhysicalUnits,
    UnresolvedLineSemantics::DrawingWcs,
    UnresolvedLineSemantics::Visibility,
    UnresolvedLineSemantics::Layout,
    UnresolvedLineSemantics::MaterialMeaning,
    UnresolvedLineSemantics::GroupMeaning,
    UnresolvedLineSemantics::FinalPrintedAppearance,
    UnresolvedLineSemantics::DocumentCompleteness,
];

/// A qualified planar primitive in its explicit stored-XY scope. The source
/// line, full ancestor evidence and file-local style indices remain available.
/// Private construction keeps the versioned admission check on the decoder path.
#[derive(Debug, Clone, PartialEq)]
pub struct Line2D {
    scene: SceneLine2D,
    profile: Line2DProfile,
}

impl Line2D {
    #[must_use]
    pub const fn profile(&self) -> Line2DProfile {
        self.profile
    }

    #[must_use]
    pub const fn coordinate_semantics(&self) -> &'static str {
        "stored_xy"
    }

    #[must_use]
    pub const fn units(&self) -> Option<&'static str> {
        None
    }

    #[must_use]
    pub const fn unresolved_semantics(&self) -> &'static [UnresolvedLineSemantics] {
        &LINE2D_UNRESOLVED_SEMANTICS
    }

    #[must_use]
    pub const fn scene(&self) -> &SceneLine2D {
        &self.scene
    }

    #[must_use]
    pub fn into_scene(self) -> SceneLine2D {
        self.scene
    }
}

/// Successful admission of the selected root subtree, containing at least one
/// Line2D. This is not a Document, a visibility result, or a complete drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedLineSelection {
    record_index: u64,
    profile: Line2DProfile,
    lines: Vec<Line2D>,
}

impl QualifiedLineSelection {
    #[must_use]
    pub const fn record_index(&self) -> u64 {
        self.record_index
    }

    #[must_use]
    pub const fn profile(&self) -> Line2DProfile {
        self.profile
    }

    #[must_use]
    pub const fn coordinate_semantics(&self) -> &'static str {
        "stored_xy"
    }

    #[must_use]
    pub const fn document_complete(&self) -> bool {
        false
    }

    #[must_use]
    pub fn lines(&self) -> &[Line2D] {
        &self.lines
    }

    #[must_use]
    pub fn into_lines(self) -> Vec<Line2D> {
        self.lines
    }
}

fn unsupported(message: &str) -> DracadError {
    DracadError::UnsupportedFeature(message.into())
}

fn span(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[8], bytes[9]])
}

fn ancestor_attributes(ancestor: &SceneAncestor, allow_inline: bool) -> Result<RecordAttributes> {
    let bytes = &ancestor.raw_bytes;
    let mut header: [u8; 16] = bytes
        .get(..16)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("truncated qualified ancestor header"))?;
    let tag = u16::from_le_bytes([header[0], header[1]]);
    match ancestor.kind {
        SceneAncestorKind::InlinePackage => {
            if !(matches!(tag, 0x30 | 0x430) || (allow_inline && matches!(tag, 0x2030 | 0x4030)))
                || bytes.len() != 64
                || span(bytes) < 2
                || bytes[16..20] != 0x400u32.to_le_bytes()
                || bytes[20..].iter().any(|b| *b != 0)
                || ancestor.target_definition.is_some()
            {
                return Err(unsupported("inline ancestor is outside Line2D v1"));
            }
            if tag == 0x2030 {
                header[..2].copy_from_slice(&0x2402u16.to_le_bytes());
            }
            if tag == 0x4030 {
                header[..2].copy_from_slice(&0x4402u16.to_le_bytes());
            }
        }
        SceneAncestorKind::BlockReference => {
            if !matches!(tag, 6 | 0x2006 | 0x4006)
                || bytes.len() != 128
                || span(bytes) != 2
                || bytes[64..66] != 0xfeu16.to_le_bytes()
            {
                return Err(unsupported("reference ancestor is outside Line2D v1"));
            }
            let floats = |offset: usize| -> Result<f64> {
                let value = f64::from_le_bytes(
                    bytes[offset..offset + 8]
                        .try_into()
                        .map_err(|_| invalid("truncated qualified reference numeric field"))?,
                );
                if !value.is_finite() {
                    return Err(invalid("non-finite qualified reference field"));
                }
                Ok(value)
            };
            let placement = (0..6)
                .map(|i| floats(80 + i * 8))
                .collect::<Result<Vec<_>>>()?;
            if floats(16)? != 0.0
                || floats(24)? != 0.0
                || placement[2] != 0.0
                || placement[3] <= 0.0
                || placement[3] != placement[4]
            {
                return Err(unsupported("reference placement is outside Line2D v1"));
            }
            let definition = ancestor
                .target_definition
                .as_ref()
                .ok_or_else(|| invalid("qualified reference lacks definition evidence"))?;
            if definition.header_bytes[40..112]
                .iter()
                .chain(&definition.header_bytes[160..176])
                .any(|b| *b != 0)
            {
                return Err(unsupported(
                    "definition coordinate fields are outside Line2D v1",
                ));
            }
            // Exact observed selector combinations; do not mask arbitrary flags.
            if tag == 0x2006 {
                header[..2].copy_from_slice(&0x2402u16.to_le_bytes());
            }
            if tag == 0x4006 {
                header[..2].copy_from_slice(&0x4402u16.to_le_bytes());
            }
        }
    }
    crate::decode_record_attributes(&header)
}

fn check_dependency_scope(
    attributes: &RecordAttributes,
    inline_since_reference: bool,
) -> Result<()> {
    if inline_since_reference && attributes.has_dependencies() {
        return Err(unsupported(
            "inline-dependent attributes are outside Line2D v1",
        ));
    }
    Ok(())
}

fn admit_v1(line: &SceneLine2D, allow_inline: bool) -> Result<()> {
    if !matches!(line.source.tag_raw, 2 | 0x402 | 0x2402 | 0x4402)
        || span(&line.source.header_bytes) != 1
        || line.source.elevation != 0.0
        || line.source.thickness != 0.0
    {
        return Err(unsupported("line fields are outside Line2D v1"));
    }
    if !line
        .source
        .start
        .into_iter()
        .chain(line.source.end)
        .chain(line.start)
        .chain(line.end)
        .chain(line.cumulative_transform)
        .all(f64::is_finite)
    {
        return Err(invalid("non-finite qualified line coordinates"));
    }
    if line.source.start == line.source.end {
        return Err(unsupported(
            "source zero-length line has no persisted native Line2D v1 evidence",
        ));
    }
    if line.ancestors.len() as u64 > LINE2D_V1_MAX_ANCESTORS {
        return Err(unsupported("ancestor depth exceeds Line2D v1 native scope"));
    }
    let mut references = 0;
    let mut inlines = 0;
    let mut inline_since_reference = false;
    for ancestor in &line.ancestors {
        let attributes = ancestor_attributes(ancestor, allow_inline)?;
        match ancestor.kind {
            SceneAncestorKind::InlinePackage => {
                inlines += 1;
                // Native v1 controls qualify explicit wrappers and explicit
                // children only. A later scene extension cannot widen this.
                if !allow_inline && attributes.has_dependencies() {
                    return Err(unsupported("dependent inline wrapper is outside Line2D v1"));
                }
                inline_since_reference = true;
            }
            SceneAncestorKind::BlockReference => {
                references += 1;
                if !allow_inline {
                    check_dependency_scope(&attributes, inline_since_reference)?;
                }
                inline_since_reference = false;
            }
        }
    }
    if references > LINE2D_V1_MAX_REFERENCE_DEPTH || inlines > LINE2D_V1_MAX_INLINE_DEPTH {
        return Err(unsupported(
            "reference/inline depth exceeds Line2D v1 native scope",
        ));
    }
    if !allow_inline {
        check_dependency_scope(&line.attributes, inline_since_reference)?;
    }
    // Native ByBlock requires an insertion scope. The scene reader also checks
    // each source chain; this preserves the contract if that reader expands.
    if references == 0
        && [
            line.attributes.color,
            line.attributes.line_type,
            line.attributes.width,
        ]
        .iter()
        .any(|r| r.kind == AttributeReferenceKind::ByBlock)
    {
        return Err(unsupported("Line2D v1 ByBlock requires an insertion scope"));
    }
    Ok(())
}

fn numerical(value: f64) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid("non-finite qualified placement arithmetic"))
    }
}

fn f64_field(bytes: &[u8], offset: usize) -> Result<f64> {
    let field = bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| invalid("truncated qualified reference numeric field"))?;
    numerical(f64::from_le_bytes(field))
}

fn product(left: f64, right: f64) -> Result<f64> {
    let result = numerical(left * right)?;
    if left != 0.0 && right != 0.0 && result == 0.0 {
        return Err(invalid("qualified placement nonzero product underflow"));
    }
    Ok(result)
}

fn translated(relative: f64, origin: f64) -> Result<f64> {
    let result = numerical(relative + origin)?;
    if (relative != 0.0 && result == origin) || (origin != 0.0 && result == relative) {
        return Err(invalid("qualified placement translation lost precision"));
    }
    Ok(result)
}

fn cardinal_sin_cos(degrees: f64) -> Result<(f64, f64)> {
    // A nominal 90-degree turn has no tiny orthogonal displacement to preserve.
    // The source angle is retained verbatim in the ancestor evidence.
    // Signed remainder also preserves tiny negative angles: rem_euclid can
    // round those to 360.0 before their representable effect is evaluated.
    let reduced = degrees % 360.0;
    let result = match reduced {
        0.0 => (0.0, 1.0),
        90.0 | -270.0 => (1.0, 0.0),
        180.0 | -180.0 => (0.0, -1.0),
        270.0 | -90.0 => (-1.0, 0.0),
        _ => {
            let radians = numerical(reduced.to_radians())?;
            if radians == 0.0 {
                return Err(invalid("qualified nonzero angle conversion underflow"));
            }
            let result = radians.sin_cos();
            if result.0 == 0.0 {
                return Err(invalid("qualified noncardinal angle sine underflow"));
            }
            result
        }
    };
    Ok(result)
}

fn linear(point: [f64; 2], scale: f64, sine: f64, cosine: f64) -> Result<[f64; 2]> {
    let rotated = [
        numerical(product(point[0], cosine)? - product(point[1], sine)?)?,
        numerical(product(point[0], sine)? + product(point[1], cosine)?)?,
    ];
    if point != [0.0; 2] && rotated == [0.0; 2] {
        return Err(invalid("qualified placement rotation lost displacement"));
    }
    Ok([product(rotated[0], scale)?, product(rotated[1], scale)?])
}

fn placed(point: [f64; 2], placement: [f64; 6]) -> Result<[f64; 2]> {
    let [tx, ty, _, scale, _, degrees] = placement;
    let (sine, cosine) = cardinal_sin_cos(degrees)?;
    let relative = linear(point, scale, sine, cosine)?;
    Ok([translated(relative[0], tx)?, translated(relative[1], ty)?])
}

fn composed(parent: [f64; 6], local: [f64; 6]) -> Result<[f64; 6]> {
    let [a, b, c, d, x, y] = parent;
    let [e, f, g, h, u, v] = local;
    let result = [
        numerical(product(a, e)? + product(c, f)?)?,
        numerical(product(b, e)? + product(d, f)?)?,
        numerical(product(a, g)? + product(c, h)?)?,
        numerical(product(b, g)? + product(d, h)?)?,
        translated(numerical(product(a, u)? + product(c, v)?)?, x)?,
        translated(numerical(product(b, u)? + product(d, v)?)?, y)?,
    ];
    if result[..4].iter().all(|value| *value == 0.0) {
        return Err(invalid("qualified composed scale underflow"));
    }
    Ok(result)
}

/// Re-evaluate only the qualified path. The raw/scene API retains its existing
/// arithmetic; source records, ancestor bytes and style evidence are untouched.
fn qualified_coordinates(line: &mut SceneLine2D) -> Result<()> {
    let mut start = line.source.start;
    let mut end = line.source.end;
    let mut matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    for ancestor in line
        .ancestors
        .iter()
        .filter(|value| value.kind == SceneAncestorKind::BlockReference)
        .rev()
    {
        let mut placement = [0.0; 6];
        for (index, value) in placement.iter_mut().enumerate() {
            *value = f64_field(&ancestor.raw_bytes, 80 + index * 8)?;
        }
        let [tx, ty, _, scale, _, degrees] = placement;
        let (sine, cosine) = cardinal_sin_cos(degrees)?;
        let displacement = [numerical(end[0] - start[0])?, numerical(end[1] - start[1])?];
        let expected = linear(displacement, scale, sine, cosine)?;
        start = placed(start, placement)?;
        end = placed(end, placement)?;
        for axis in 0..2 {
            if expected[axis] != 0.0 && numerical(end[axis] - start[axis])? == 0.0 {
                return Err(invalid(
                    "qualified line placement lost endpoint displacement",
                ));
            }
        }
        let local = [
            product(scale, cosine)?,
            product(scale, sine)?,
            -product(scale, sine)?,
            product(scale, cosine)?,
            tx,
            ty,
        ];
        // Traversal here is inner-to-outer, so the newly encountered placement
        // is the parent of the matrix accumulated so far.
        matrix = composed(local, matrix)?;
    }
    line.start = start;
    line.end = end;
    line.cumulative_transform = matrix;
    Ok(())
}

/// Opt into a named, versioned subset. All selected lines must pass; there is
/// no partial result, implicit physical-unit conversion, or complete Document.
pub fn qualify_lines(
    document: &RawDocument,
    record_index: u64,
    profile: Line2DProfile,
    limits: &SceneLimits,
) -> Result<QualifiedLineSelection> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported("Line2D v1 requires observed marker25"));
    }
    let mut scenes = match profile {
        Line2DProfile::Marker25PlanarStoredXyV1 => {
            crate::scene::decode(document, record_index, limits)?
        }
        Line2DProfile::Marker25PlanarStoredXyInlineV1 => {
            crate::scene::decode_with_inline_scopes(document, record_index, limits)?
        }
    };
    admit_selection(record_index, profile, &mut scenes)
}

pub(crate) fn qualify_selected(
    document: &RawDocument,
    span: &crate::RawObjectSpan,
    profile: Line2DProfile,
    limits: &SceneLimits,
    context: &mut crate::scene::DecodeContext,
) -> Result<QualifiedLineSelection> {
    if document.header().profile != ObservedProfile::Marker25 {
        return Err(unsupported("Line2D v1 requires observed marker25"));
    }
    let mut scenes = crate::scene::decode_selected(
        document,
        span.record_index,
        span.end_record_index,
        limits,
        profile == Line2DProfile::Marker25PlanarStoredXyInlineV1,
        context,
    )?;
    admit_selection(span.record_index, profile, &mut scenes)
}

fn admit_selection(
    record_index: u64,
    profile: Line2DProfile,
    scenes: &mut Vec<SceneLine2D>,
) -> Result<QualifiedLineSelection> {
    if scenes.is_empty() {
        return Err(unsupported("Line2D v1 requires at least one planar line"));
    }
    for line in &mut *scenes {
        match profile {
            Line2DProfile::Marker25PlanarStoredXyV1 => admit_v1(line, false)?,
            Line2DProfile::Marker25PlanarStoredXyInlineV1 => admit_v1(line, true)?,
        }
        qualified_coordinates(line)?;
    }
    Ok(QualifiedLineSelection {
        record_index,
        profile,
        lines: std::mem::take(scenes)
            .into_iter()
            .map(|scene| Line2D { scene, profile })
            .collect(),
    })
}
