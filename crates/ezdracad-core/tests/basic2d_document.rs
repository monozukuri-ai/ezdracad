//! Independently authored synthetic behavior tests, never native proof.
use ezdracad_core::{
    read, root_curve_2d, root_text_2d, Basic2DProfile, DracadError, Entity, InlineTableDomain,
    ReadOptions, ReadPolicy, ReadScope, ReadStatus, StyleSourceKind,
};
use std::io::{Cursor, Write};

const MPZ: &[u8] = include_bytes!("../../../tests/fixtures/basic2d25.cfb");
const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/basic2d25.bin");
const START: usize = 8192;
const ROOT_INDICES: [u64; 7] = [2, 3, 5, 7, 12, 18, 20];

fn options() -> ReadOptions {
    ReadOptions {
        scope: ReadScope::StoredXyPrimitives,
        basic_profile: Some(Basic2DProfile::Marker25RootPrimitivesV1),
        ..ReadOptions::default()
    }
}

fn partial() -> ReadOptions {
    ReadOptions {
        policy: ReadPolicy::Partial,
        ..options()
    }
}

fn wrap(contents: &[u8]) -> Vec<u8> {
    let mut compound = cfb::CompoundFile::open(Cursor::new(MPZ.to_vec())).unwrap();
    {
        let mut stream = compound.open_stream("/Contents").unwrap();
        stream.set_len(contents.len() as u64).unwrap();
        stream.write_all(contents).unwrap();
    }
    compound.into_inner().into_inner()
}

fn rebuilt(records: &[u8]) -> Vec<u8> {
    assert_eq!(records.len() % 64, 0);
    let old_end = u32::from_le_bytes(CONTENTS[0x28..0x2c].try_into().unwrap()) as usize;
    let new_end = START + records.len() + 64;
    let delta = new_end as i64 - old_end as i64;
    let mut result = CONTENTS[..START].to_vec();
    result.extend_from_slice(records);
    result.push(0xff);
    result.resize(new_end, 0);
    result.extend_from_slice(&CONTENTS[old_end..]);
    for at in (0x28..0x54).step_by(4) {
        let old = u32::from_le_bytes(CONTENTS[at..at + 4].try_into().unwrap());
        if old != 0 {
            result[at..at + 4].copy_from_slice(&((i64::from(old) + delta) as u32).to_le_bytes());
        }
    }
    result[0x58..0x5c].copy_from_slice(&((records.len() / 64) as u32).to_le_bytes());
    result
}

fn changed(index: usize, within: usize, value: &[u8]) -> Vec<u8> {
    let mut contents = CONTENTS.to_vec();
    let at = START + index * 64 + within;
    contents[at..at + value.len()].copy_from_slice(value);
    wrap(&contents)
}

#[test]
fn strict_narrow_request_decodes_mixed_types_without_claiming_document_completeness() {
    let doc = read(MPZ, &options()).unwrap();
    let report = doc.report();
    assert_eq!(report.status, ReadStatus::Complete);
    assert_eq!(report.geometry_status, ReadStatus::Complete);
    assert_eq!(report.document_status, ReadStatus::Partial);
    assert!(!report.document_complete());
    assert_eq!(report.units(), None);
    assert_eq!(report.coordinate_semantics(), "stored_xy");
    assert_eq!(
        report.requested_entity_types(),
        ["LINE", "CIRCLE", "ARC", "TEXT"]
    );
    assert_eq!(report.raw_record_count, 26);
    assert_eq!(report.primary_object_count, 9);
    assert_eq!(report.interior_record_count, 16);
    assert_eq!(report.auxiliary_record_count, 16);
    assert_eq!(report.handled_auxiliary_object_count, 2);
    assert_eq!(report.decoded_object_count, 7);
    assert_eq!(report.entity_count, 7);
    assert!(report.unsupported_counts.is_empty());
    assert!(report.context_blocked_counts.is_empty());
    assert_eq!(doc.raw().contents(), CONTENTS);
    assert_eq!(report.unresolved_semantics().len(), 8);
    let auxiliaries: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "recognized_auxiliary_property")
        .collect();
    assert_eq!(auxiliaries.len(), 2);
    assert_eq!(auxiliaries[0].source.as_ref().unwrap().record_index, 0);
    assert_eq!(auxiliaries[1].source.as_ref().unwrap().record_index, 21);
}

#[test]
fn entity_geometry_and_raw_primitive_provenance_keep_exact_source_locations() {
    let doc = read(MPZ, &options()).unwrap();
    for (id, entity) in doc.entities().iter().enumerate() {
        assert_eq!(entity.entity_id(), id as u64);
        assert_eq!(entity.root_record_index(), ROOT_INDICES[id]);
        let source = entity.source_ref();
        assert_eq!(source.record_index, ROOT_INDICES[id]);
        assert_eq!(source.payload_offset, 8192 + 64 * ROOT_INDICES[id]);
        assert_eq!(source.table_domain, InlineTableDomain::RootRecords);
        assert_eq!(source.definition_index, None);
        assert_eq!(source.stream_path.as_deref(), Some("/Contents"));
        assert_eq!(source.offset_domain(), "cfb_stream");
        let start = source.payload_offset as usize;
        let raw = &CONTENTS[start..start + source.byte_length as usize];
        let (styles, attributes) = match entity {
            Entity::Line(e) => {
                assert_eq!(e.geometry.scene().start, [-1.125, 2.25]);
                assert_eq!(e.geometry.scene().end, [3.5, 4.75]);
                assert_eq!(e.geometry.units(), None);
                (&e.geometry.scene().styles, &e.geometry.scene().attributes)
            }
            Entity::Circle(e) => {
                assert_eq!(e.geometry.center, [10.25, -20.5]);
                assert_eq!(e.geometry.radius, 3.75);
                assert_eq!(e.geometry.raw_bytes, raw);
                (&e.style.styles, &e.style.attributes)
            }
            Entity::Arc(e) => {
                assert_eq!(e.geometry.center, [-3.0, 7.0]);
                assert_eq!(e.geometry.radius, 9.0);
                assert_eq!(e.geometry.start_angle_degrees, 270.0);
                assert_eq!(e.geometry.end_angle_degrees, 15.0);
                assert_eq!(e.geometry.raw_bytes, raw);
                (&e.style.styles, &e.style.attributes)
            }
            Entity::Text(e) => {
                assert_eq!(e.geometry.basepoint, [6.125, -8.25]);
                assert_eq!(e.geometry.font_name, "Synthetic Font");
                assert_eq!(e.geometry.origin, 8);
                assert_eq!(
                    e.geometry.body_encoding(),
                    "ascii_native_bmp_unicode_escapes"
                );
                assert_eq!(e.geometry.raw_bytes, raw);
                if id == 3 {
                    assert_eq!(e.geometry.text, "合成図面β");
                    assert_eq!(e.geometry.body_bytes.len(), 60);
                    assert_eq!(
                        (
                            e.geometry.width,
                            e.geometry.height,
                            e.geometry.rotation_degrees
                        ),
                        (2.5, 4.0, 17.0)
                    );
                } else {
                    assert_eq!(e.geometry.text, "A".repeat(95) + "Z");
                    assert_eq!(e.geometry.body_bytes.len(), 96);
                    assert_eq!(
                        (
                            e.geometry.width,
                            e.geometry.height,
                            e.geometry.rotation_degrees
                        ),
                        (1.75, 5.25, -23.5)
                    );
                }
                (&e.style.styles, &e.style.attributes)
            }
        };
        assert_eq!(attributes.layer_index, 2);
        assert_eq!(attributes.header_bytes, raw[..16]);
        assert_eq!(attributes.tag_raw, u16::from_le_bytes([raw[0], raw[1]]));
        assert_eq!(styles.color.index, 3);
        assert_eq!(styles.line_type.index, 2);
        assert_eq!(styles.width.index, 4);
        for resolved in [&styles.color, &styles.line_type, &styles.width] {
            let record = resolved
                .sources
                .iter()
                .find(|s| s.kind == StyleSourceKind::Record)
                .unwrap();
            assert_eq!(record.table_domain, "root_records");
            assert_eq!(record.index, ROOT_INDICES[id]);
            assert_eq!(record.payload_offset, source.payload_offset);
            assert_eq!(record.raw_bytes, raw);
        }
    }
}

#[test]
fn type_pages_preserve_global_ids_duplicates_and_distinguish_unknown_from_absent() {
    let doc = read(MPZ, &options()).unwrap();
    for (kind, ids) in [
        ("LINE", vec![0, 6]),
        ("CIRCLE", vec![1, 5]),
        ("ARC", vec![2]),
        ("TEXT", vec![3, 4]),
    ] {
        assert_eq!(doc.query_count(kind).unwrap(), ids.len() as u64);
        assert_eq!(
            doc.query(kind, 0, 4096)
                .unwrap()
                .iter()
                .map(|e| e.entity_id())
                .collect::<Vec<_>>(),
            ids
        );
        for (position, id) in ids.iter().enumerate() {
            assert_eq!(
                doc.query(kind, position as u64, 1).unwrap()[0].entity_id(),
                *id
            );
        }
        assert!(doc.query(kind, ids.len() as u64, 256).unwrap().is_empty());
        assert!(matches!(
            doc.query(kind, ids.len() as u64 + 1, 1),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    assert_eq!(
        doc.query("*", 2, 3)
            .unwrap()
            .iter()
            .map(|e| e.entity_id())
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(
        doc.query("CIRCLE", 1, 1).unwrap()[0].root_record_index(),
        18
    );
    assert!(doc.query("*", 0, 0).unwrap().is_empty());
    assert!(matches!(
        doc.query("*", 0, 4097),
        Err(DracadError::LimitExceeded { .. })
    ));
    for unknown in ["POLYLINE", "circle", "", "LINE CIRCLE"] {
        assert!(matches!(
            doc.query_count(unknown),
            Err(DracadError::UnsupportedFeature(_))
        ));
    }
    let duplicate_a = match &doc.entities()[1] {
        Entity::Circle(e) => e,
        _ => panic!(),
    };
    let duplicate_b = match &doc.entities()[5] {
        Entity::Circle(e) => e,
        _ => panic!(),
    };
    assert_eq!(
        duplicate_a.geometry.raw_bytes,
        duplicate_b.geometry.raw_bytes
    );
    assert_ne!(duplicate_a.source_ref, duplicate_b.source_ref);
}

#[test]
fn strict_drawing_and_legacy_profile_keep_their_own_contracts() {
    let error = read(
        MPZ,
        &ReadOptions {
            scope: ReadScope::Drawing,
            ..options()
        },
    )
    .unwrap_err();
    let report = error.report().unwrap();
    assert_eq!(report.status, ReadStatus::Partial);
    assert_eq!(report.geometry_status, ReadStatus::Complete);
    assert_eq!(report.entity_count, 7);
    let old = read(
        MPZ,
        &ReadOptions {
            policy: ReadPolicy::Partial,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(old.report().requested_entity_types(), ["LINE"]);
    assert_eq!(old.report().handled_auxiliary_object_count, 0);
    assert_eq!(old.report().unsupported_counts.get(&0x33), Some(&1));
    assert!(old.entities().is_empty());
    assert!(old.query_count("CIRCLE").is_err());
    for invalid in [
        ReadOptions {
            basic_profile: None,
            ..options()
        },
        ReadOptions {
            scope: ReadScope::StoredXyLines,
            ..options()
        },
    ] {
        let error = read(MPZ, &invalid).unwrap_err();
        assert!(matches!(error.error(), DracadError::UnsupportedFeature(_)));
        assert!(error.report().is_none());
    }
}

#[test]
fn unknown_curve_flags_and_unqualified_attributes_atomically_fence_later_entities() {
    for (within, value) in [
        (0, 0x8003_u16.to_le_bytes().to_vec()),
        (6, vec![0]),
        (7, vec![1]),
        (12, vec![1]),
        (32, vec![1]),
        (66, vec![1]),
    ] {
        let bytes = changed(3, within, &value);
        let doc = read(&bytes, &partial()).unwrap();
        assert_eq!(doc.entities().len(), 1);
        assert_eq!(doc.report().handled_auxiliary_object_count, 1);
        assert_eq!(doc.report().decoded_object_count, 1);
        assert_eq!(doc.report().unsupported_counts.values().sum::<u64>(), 1);
        assert_eq!(doc.report().context_blocked_counts.values().sum::<u64>(), 6);
        assert_eq!(doc.report().context_blocked_counts.get(&0x33), Some(&1));
        let strict = read(&bytes, &options()).unwrap_err();
        assert_eq!(strict.report().unwrap().entity_count, 1);
        assert_eq!(
            strict.report().unwrap().unsupported_counts,
            doc.report().unsupported_counts
        );
    }
}

#[test]
fn unknown_metadata_is_not_skipped_and_duplicate_known_properties_are_not_deduplicated() {
    for (within, value) in [(32, b'X'), (80, b'X'), (22, 1), (12, 1), (66, 1), (127, 1)] {
        let doc = read(&changed(0, within, &[value]), &partial()).unwrap();
        assert!(doc.entities().is_empty());
        assert_eq!(doc.report().handled_auxiliary_object_count, 0);
        assert_eq!(doc.report().unsupported_counts.get(&0x33), Some(&1));
        assert_eq!(doc.report().context_blocked_counts.values().sum::<u64>(), 8);
    }
    let end = START + 25 * 64;
    let records = [&CONTENTS[START..START + 128], &CONTENTS[START..end]].concat();
    let doc = read(&wrap(&rebuilt(&records)), &options()).unwrap();
    assert_eq!(doc.report().handled_auxiliary_object_count, 3);
    assert_eq!(doc.entities().len(), 7);
    assert_eq!(doc.entities()[0].root_record_index(), 4);
    let final_unknown = read(&changed(21, 32, b"X"), &partial()).unwrap();
    assert_eq!(final_unknown.entities().len(), 7);
    assert_eq!(final_unknown.report().status, ReadStatus::Partial);
    assert_eq!(
        final_unknown.report().unsupported_counts.get(&0x33),
        Some(&1)
    );
}

#[test]
fn known_numeric_corruption_and_continuation_breakage_are_fatal_after_an_unknown_state() {
    for (index, within, value) in [
        (3, 16, f64::NAN.to_le_bytes().to_vec()),
        (3, 80, f64::INFINITY.to_le_bytes().to_vec()),
        (5, 96, f64::NAN.to_le_bytes().to_vec()),
        (7, 144, f64::NAN.to_le_bytes().to_vec()),
        (12, 48, f64::INFINITY.to_le_bytes().to_vec()),
        (3, 64, 0xfdu16.to_le_bytes().to_vec()),
        (7, 64, 0xfdu16.to_le_bytes().to_vec()),
        (21, 64, 0xfdu16.to_le_bytes().to_vec()),
        (20, 8, 0u16.to_le_bytes().to_vec()),
    ] {
        let mut contents = CONTENTS.to_vec();
        contents[START + 32] = b'X'; // Unknown property holds geometry, never hides corruption.
        let at = START + index * 64 + within;
        contents[at..at + value.len()].copy_from_slice(&value);
        let error = read(&wrap(&contents), &partial()).unwrap_err();
        assert!(
            matches!(error.error(), DracadError::InvalidFormat(_)),
            "{index}+{within}: {error}"
        );
        assert!(error.report().is_none());
    }
}

#[test]
fn text_span_length_nul_and_font_corruption_are_not_partial_text_successes() {
    for (index, within, value) in [
        (7, 80, 61u16.to_le_bytes().to_vec()),
        (12, 336, vec![b'X']),
        (7, 160, 0xd800u16.to_le_bytes().to_vec()),
        (7, 8, 0u16.to_le_bytes().to_vec()),
    ] {
        let error = read(&changed(index, within, &value), &partial()).unwrap_err();
        assert!(
            matches!(error.error(), DracadError::InvalidFormat(_)),
            "{index}+{within}: {error}"
        );
        assert!(error.report().is_none());
    }
    let doc = read(&changed(7, 134, &[54]), &partial()).unwrap();
    assert_eq!(doc.entities().len(), 3);
    assert_eq!(doc.report().unsupported_counts.get(&5), Some(&1));
    assert_eq!(doc.report().context_blocked_counts.get(&5), Some(&1));
}

#[test]
fn text_limits_and_aggregate_budgets_are_fatal_and_diagnostics_have_independent_caps() {
    for options in [
        {
            let mut o = partial();
            o.text_limits.max_encoded_bytes = 95;
            o
        },
        {
            let mut o = partial();
            o.text_limits.max_output_bytes = 95;
            o
        },
        {
            let mut o = partial();
            o.read_limits.max_objects = 8;
            o
        },
        {
            let mut o = partial();
            o.read_limits.max_entities = 6;
            o
        },
        {
            let mut o = partial();
            o.read_limits.max_nodes = 0;
            o
        },
        {
            let mut o = partial();
            o.read_limits.max_provenance_bytes = 0;
            o
        },
    ] {
        // The same checks apply despite an earlier unknown metadata owner.
        let bytes = if options.text_limits.max_encoded_bytes == 95
            || options.text_limits.max_output_bytes == 95
        {
            changed(0, 32, b"X")
        } else {
            MPZ.to_vec()
        };
        let error = read(&bytes, &options).unwrap_err();
        assert!(
            matches!(error.error(), DracadError::LimitExceeded { .. }),
            "{error}"
        );
        assert!(error.report().is_none());
    }
    let mut options = options();
    options.read_limits.max_diagnostics = 0;
    let doc = read(MPZ, &options).unwrap();
    assert!(doc.report().diagnostics.is_empty());
    assert!(doc.report().diagnostics_suppressed >= 10);
    assert_eq!(doc.report().handled_auxiliary_object_count, 2);
}

#[test]
fn root_only_raw_apis_reject_interiors_and_preserve_full_selected_spans() {
    let doc = read(MPZ, &options()).unwrap();
    let profile = Basic2DProfile::Marker25RootPrimitivesV1;
    let curve = root_curve_2d(doc.raw(), 3, profile).unwrap();
    assert_eq!(curve.record_index(), 3);
    assert_eq!(curve.payload_offset(), 8384);
    assert_eq!(curve.raw_bytes(), &CONTENTS[8384..8512]);
    assert_eq!(curve.coordinate_scope(), "root_records");
    assert_eq!(curve.units(), None);
    assert!(root_curve_2d(doc.raw(), 4, profile).is_err());
    assert!(root_curve_2d(doc.raw(), 25, profile).is_err());
    let text = root_text_2d(doc.raw(), 7, &options().text_limits).unwrap();
    assert_eq!(text.record_index, 7);
    assert_eq!(text.raw_bytes.len(), 320);
    assert_eq!(text.units(), None);
    assert!(root_text_2d(doc.raw(), 8, &options().text_limits).is_err());
    assert!(Basic2DProfile::from_id("guessed").is_err());
}

#[test]
fn public_curve_selection_owns_bytes_and_validates_even_later_root_spans() {
    let profile = Basic2DProfile::Marker25RootPrimitivesV1;
    let retained = {
        let doc = read(MPZ, &options()).unwrap();
        root_curve_2d(doc.raw(), 3, profile).unwrap()
    };
    assert_eq!(retained.raw_bytes(), &CONTENTS[8384..8512]);
    let raw =
        ezdracad_core::scan_contents(CONTENTS, &ezdracad_core::ParseLimits::default()).unwrap();
    assert!(matches!(
        root_curve_2d(&raw, 999, profile),
        Err(DracadError::UnsupportedFeature(_))
    ));
    let mut contents = CONTENTS.to_vec();
    contents[START + 21 * 64 + 8..START + 21 * 64 + 10].copy_from_slice(&0u16.to_le_bytes());
    let raw =
        ezdracad_core::scan_contents(&contents, &ezdracad_core::ParseLimits::default()).unwrap();
    assert!(matches!(
        root_curve_2d(&raw, 3, profile),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn unsupported_primitive_selections_consume_the_work_budget() {
    for records in [
        {
            let mut r = CONTENTS[START + 3 * 64..START + 5 * 64].to_vec();
            r[80..88].copy_from_slice(&0f64.to_le_bytes());
            r
        },
        {
            let mut r = CONTENTS[START + 7 * 64..START + 12 * 64].to_vec();
            r[134] = 0;
            r
        },
    ] {
        let data = wrap(&rebuilt(&records));
        let doc = read(&data, &partial()).unwrap();
        assert_eq!(doc.report().entity_count, 0);
        assert_eq!(doc.report().scene_nodes_visited, 1);
        assert_eq!(doc.report().unsupported_counts.values().sum::<u64>(), 1);
        let mut opts = partial();
        opts.read_limits.max_nodes = 0;
        let error = read(&data, &opts).unwrap_err();
        assert!(matches!(
            error.error(),
            DracadError::LimitExceeded {
                resource: "document_nodes",
                ..
            }
        ));
        assert!(error.report().is_none());
    }
}
