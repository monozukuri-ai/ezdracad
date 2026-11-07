#![no_main]

use ezdracad_core::{
    coordinate_space_context, qualify_lines_in_coordinate_space, resolve_nominal_print_width,
    scan_contents, workplane_context, world_input_3d_context, Block3DProfile, CoordinateFrame,
    CoordinateSpaceProfile, DeclaredCreationScale, EffectiveStyleIndices, InlinePackageLimits,
    Line2DProfile, ObservedProfile, ParseLimits, PrintDeviceContext, RawStyleTables, SceneLimits,
    WorkplaneProfile, WorldInput3DProfile,
};
use libfuzzer_sys::fuzz_target;

fn check_styles(styles: &EffectiveStyleIndices, data: &[u8]) {
    for value in [&styles.color, &styles.line_type, &styles.width] {
        assert!(value.index > 0 && value.index <= 256);
        assert!((2..=4).contains(&value.sources.len()));
        for source in &value.sources {
            let start = source.payload_offset as usize;
            assert_eq!(
                source.raw_bytes,
                data[start..start + source.raw_bytes.len()]
            );
        }
    }
    assert!(styles.line_type.index <= 32 && styles.width.index <= 16);
}

fuzz_target!(|data: &[u8]| {
    let limits = ParseLimits {
        max_stream_size: 128 * 1024,
        max_records: 2048,
        ..ParseLimits::default()
    };
    if let Ok(doc) = scan_contents(data, &limits) {
        for index in 0..doc.header().record_count.min(4) {
            if let Ok(selection) =
                doc.placed_block_lines_3d(index, Block3DProfile::Marker25DirectLine3dV1, 8)
            {
                assert!(!selection.document_complete());
                assert_eq!(selection.units(), None);
                assert_eq!(selection.contents_sha256(), doc.sha256());
                let evidence = selection.reference().evidence();
                for (record, offset) in evidence.records.iter().zip(evidence.payload_offsets) {
                    assert_eq!(record, &data[offset as usize..offset as usize + 64]);
                }
                let definition = selection.definition();
                assert_eq!(
                    definition.header_bytes,
                    data[definition.header_offset as usize
                        ..definition.header_offset as usize + 176]
                );
                assert!(selection.lines().len() <= 8);
                for line in selection.lines() {
                    assert!(line.start.into_iter().chain(line.end).all(f64::is_finite));
                    assert_eq!(
                        line.source.header_bytes,
                        data[line.source.payload_offset as usize
                            ..line.source.payload_offset as usize + 16]
                    );
                    assert_eq!(
                        line.start,
                        selection.reference().apply_xyz(line.source.start).unwrap()
                    );
                }
            }
        }
        if let Ok(value) = doc.dimension_scale(
            0,
            ezdracad_core::DimensionScaleProfile::Marker25AlignedNumericMmV1,
        ) {
            let start = value.payload_offset as usize;
            assert_eq!(value.record_bytes, data[start..start + 640]);
            assert!(value.annotation_to_drawing_ratio.is_finite());
            assert!(value.annotation_to_drawing_ratio > 0.0);
            assert!(value.equivalent_annotation_denominator.is_finite());
            assert_eq!(value.original_drawing_denominator(), None);
            assert_eq!(value.original_annotation_denominator(), None);
        }
        let space_profile = match doc.header().profile {
            ObservedProfile::Marker5 => CoordinateSpaceProfile::Marker5StoredAxesV1,
            ObservedProfile::Marker8 => CoordinateSpaceProfile::Marker8StoredAxesV1,
            ObservedProfile::Marker25 => CoordinateSpaceProfile::Marker25StoredAxesV1,
        };
        if let Ok(context) = coordinate_space_context(&doc, space_profile) {
            assert_eq!(context.evidence().header_bytes, data[0x140..0x280]);
            assert_eq!(context.evidence().contents_sha256, doc.sha256());
            for value in [0.0, -1.0, 1000.0, f64::MAX] {
                if let Ok(converted) = context.input_unit().to_native_mm(value) {
                    assert!(converted.is_finite());
                }
                if let Ok(converted) = context.input_unit().from_native_mm(value) {
                    assert!(converted.is_finite());
                }
            }
            for frame in [
                CoordinateFrame::RootStoredXy,
                CoordinateFrame::DrawingOriginRelativeStoredXy,
                CoordinateFrame::CurrentAnnotationMmFromDrawingOrigin,
                CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
                CoordinateFrame::ConfiguredPaperMmOffsetFromDrawingOrigin,
            ] {
                for point in [[0.0, 0.0], [1.0, -1.0], [f64::MAX, f64::MIN_POSITIVE]] {
                    if let Ok(value) = context.from_root_stored_xy(point, frame) {
                        assert!(value.into_iter().all(f64::is_finite));
                        let _ = context.to_root_stored_xy(value, frame);
                    }
                }
            }
        }
        if let Ok(resolution) = doc.unit_resolution() {
            assert_eq!(resolution.origin_units.selector_bytes, data[0x150..0x154]);
            assert_eq!(
                resolution.coordinate_input_unit.origin_units(),
                resolution.origin_units
            );
            if let Ok(declared) = DeclaredCreationScale::new(
                resolution.current_measurement.drawing_denominator,
                resolution.current_measurement.annotation_denominator,
            ) {
                for value in [0.0, 1.0, 1000.0, f64::MAX] {
                    if let Ok(converted) = declared.length_mm(value) {
                        assert!(converted.is_finite() && converted >= 0.0);
                    }
                }
            }
        }
        if let Ok(context) = workplane_context(&doc, WorkplaneProfile::Marker25SavedWorkplaneV1) {
            let evidence = context.evidence();
            assert_eq!(evidence.contents_sha256, doc.sha256());
            assert_eq!(evidence.header_offset, 0x450);
            assert_eq!(evidence.header_bytes, data[0x450..0x4d0]);
            assert_eq!(evidence.coordinate_guard.header_bytes, data[0x140..0x280]);
            for point in [
                [0., 0., 0.],
                [1., -2., 3.],
                [f64::MAX, f64::MIN_POSITIVE, 0.],
            ] {
                if let Ok(value) = context.from_world_stored_xyz(point) {
                    assert!(value.into_iter().all(f64::is_finite));
                    if let Ok(inverse) = context.to_world_stored_xyz(value) {
                        assert!(inverse.into_iter().all(f64::is_finite));
                    }
                }
            }
        }
        if let Ok(context) =
            world_input_3d_context(&doc, WorldInput3DProfile::Marker25LoadedWorldInputV1)
        {
            assert_eq!(context.evidence().header_bytes, data[0x140..0x280]);
            assert_eq!(context.evidence().contents_sha256, doc.sha256());
            assert_eq!(context.draw_origin_raw(), 0);
            assert_eq!(context.draw_origin_bytes(), data[0x154..0x158]);
            assert_eq!(context.loaded_world_origin()[2], 0.);
            assert_eq!(context.origin_z_source(), "loaded_file_zero_z_convention");
            for point in [
                [0., 0., 0.],
                [1., -2., 3.],
                [f64::MAX, f64::MIN_POSITIVE, -f64::MAX],
            ] {
                if let Ok(input) = context.input_mm_from_world_stored_xyz(point) {
                    assert!(input.into_iter().all(f64::is_finite));
                }
                if let Ok(stored) = context.world_stored_xyz_from_input_mm(point) {
                    assert!(stored.into_iter().all(f64::is_finite));
                }
            }
        }
        assert_eq!(doc.contents(), data);
        assert_eq!(
            doc.tag_counts().values().sum::<u64>(),
            doc.header().record_count
        );
        assert_eq!(
            doc.line_candidates().count() as u64,
            doc.line_candidate_count()
        );
        for line in doc.line_candidates() {
            assert!(line.start.into_iter().chain(line.end).all(f64::is_finite));
            assert_eq!(
                doc.record(line.record_index).unwrap().payload_offset,
                line.payload_offset
            );
        }
        for decoded in doc.line_records() {
            if let Ok(line) = decoded {
                assert!(matches!(line.tag_raw, 0x0002 | 0x0402));
                assert!(line
                    .start
                    .into_iter()
                    .chain(line.end)
                    .chain([line.elevation, line.thickness])
                    .all(f64::is_finite));
                assert_eq!(line.coordinate_space(), "record_local");
                assert_eq!(
                    doc.record(line.record_index).unwrap().bytes()[..16],
                    line.header_bytes
                );
            } else {
                assert!(matches!(
                    decoded,
                    Err(ezdracad_core::DracadError::InvalidFormat(_))
                ));
            }
        }
        match doc.line_records_3d() {
            Ok(lines) => {
                assert!(matches!(
                    doc.header().profile,
                    ObservedProfile::Marker5 | ObservedProfile::Marker25
                ));
                for decoded in lines {
                    match decoded {
                        Ok(line) => {
                            assert_eq!(line.tag_raw, 0x0013);
                            assert_eq!(line.coordinate_space(), "record_local");
                            let source = doc.record(line.record_index).unwrap();
                            assert_eq!(source.payload_offset, line.payload_offset);
                            assert_eq!(source.bytes()[..16], line.header_bytes);
                            for (value, raw) in line
                                .start
                                .into_iter()
                                .chain(line.end)
                                .zip(source.bytes()[16..].chunks_exact(8))
                            {
                                assert!(value.is_finite());
                                assert_eq!(value.to_le_bytes(), raw);
                            }
                        }
                        Err(error) => assert!(matches!(
                            error,
                            ezdracad_core::DracadError::InvalidFormat(_)
                        )),
                    }
                }
            }
            Err(error) => {
                assert_eq!(doc.header().profile, ObservedProfile::Marker8);
                assert!(matches!(
                    error,
                    ezdracad_core::DracadError::UnsupportedFeature(_)
                ));
            }
        }
        for line in doc.attributed_line_records().flatten() {
            assert!(matches!(line.source.tag_raw, 2 | 0x402 | 0x2402 | 0x4402));
            assert_eq!(line.source.header_bytes, line.attributes.header_bytes);
            assert!(line
                .source
                .start
                .into_iter()
                .chain(line.source.end)
                .chain([line.source.elevation, line.source.thickness])
                .all(f64::is_finite));
        }
        let mut next_index = 0;
        let mut effective_budget = 4;
        for decoded in doc.object_spans() {
            match decoded {
                Ok(span) => {
                    assert_eq!(span.record_index, next_index);
                    assert!(span.record_count > 0);
                    assert!(span.end_record_index < doc.header().record_count);
                    assert_eq!(span.end_record_index, span.record_index + span.record_count);
                    assert_eq!(
                        doc.record(span.record_index).unwrap().bytes()[..16],
                        span.header_bytes
                    );
                    next_index = span.end_record_index;
                    if effective_budget > 0 {
                        effective_budget -= 1;
                        if let Ok(framed) = qualify_lines_in_coordinate_space(
                            &doc,
                            span.record_index,
                            Line2DProfile::Marker25PlanarStoredXyInlineV1,
                            space_profile,
                            CoordinateFrame::CurrentAbsoluteInputMmFromDrawingOrigin,
                            &SceneLimits {
                                max_depth: 16,
                                max_nodes: 64,
                                max_lines: 32,
                            },
                        ) {
                            assert!(!framed.document_complete());
                            assert_eq!(framed.source().lines().len(), framed.coordinates().len());
                            for coordinate in framed.coordinates() {
                                assert!(coordinate
                                    .start
                                    .into_iter()
                                    .chain(coordinate.end)
                                    .all(f64::is_finite));
                            }
                        }
                        for profile in [
                            Line2DProfile::Marker25PlanarStoredXyV1,
                            Line2DProfile::Marker25PlanarStoredXyInlineV1,
                        ] {
                            if let Ok(selection) = doc.qualified_lines(
                                span.record_index,
                                profile,
                                &SceneLimits {
                                    max_depth: 16,
                                    max_nodes: 64,
                                    max_lines: 32,
                                },
                            ) {
                                assert!(!selection.document_complete());
                                assert!(!selection.lines().is_empty());
                                for line in selection.lines() {
                                    assert_eq!(line.units(), None);
                                    assert_eq!(line.coordinate_semantics(), "stored_xy");
                                    assert!(line.scene().ancestors.len() <= 3);
                                }
                            }
                        }
                        if let Ok(line) = doc.effective_line_style(span.record_index) {
                            assert_eq!(line.source.header_bytes, line.attributes.header_bytes);
                            check_styles(&line.styles, data);
                        }
                        if let Ok(lines) = doc.scene_lines(
                            span.record_index,
                            &SceneLimits {
                                max_depth: 16,
                                max_nodes: 64,
                                max_lines: 32,
                            },
                        ) {
                            for line in lines {
                                assert!(line.ancestors.len() <= 16);
                                assert!(line
                                    .start
                                    .into_iter()
                                    .chain(line.end)
                                    .chain(line.cumulative_transform)
                                    .all(f64::is_finite));
                                for chain in [
                                    &line.styles.color,
                                    &line.styles.line_type,
                                    &line.styles.width,
                                ] {
                                    for source in &chain.sources {
                                        let at = source.payload_offset as usize;
                                        assert_eq!(
                                            source.raw_bytes,
                                            data[at..at + source.raw_bytes.len()]
                                        );
                                    }
                                }
                                if let (Ok(settings), Ok(tables)) =
                                    (doc.print_settings(), RawStyleTables::parse(&doc))
                                {
                                    if let Ok(policy) = settings.width_policy(
                                        PrintDeviceContext::ApplicationManagedOpaqueVectors,
                                        None,
                                    ) {
                                        if let Ok(width) = resolve_nominal_print_width(
                                            &line.styles,
                                            &tables,
                                            &policy,
                                        ) {
                                            if let Some(mm) = width.nominal_paper_mm {
                                                assert!(mm.is_finite() && mm > 0.0);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if matches!(span.tag_raw, 0x30 | 0x430) {
                        let inline_limits = InlinePackageLimits {
                            max_depth: 16,
                            max_nodes: 64,
                            max_lines: 32,
                        };
                        if let Ok(lines) =
                            doc.inline_package_lines_with_limits(span.record_index, &inline_limits)
                        {
                            for line in lines {
                                assert!(!line.ancestors.is_empty() && line.ancestors.len() <= 16);
                                assert_eq!(
                                    line.source.header_bytes,
                                    doc.record(line.source.record_index).unwrap().bytes()[..16]
                                );
                                assert!(line
                                    .source
                                    .start
                                    .into_iter()
                                    .chain(line.source.end)
                                    .all(f64::is_finite));
                            }
                        }
                    }
                }
                Err(ezdracad_core::DracadError::InvalidFormat(_)) => break,
                Err(error) => panic!("unexpected span error: {error}"),
            }
        }
        if let Ok(settings) = doc.drawing_settings() {
            assert_eq!(settings.header_bytes, data[0x140..0x180]);
            assert!(settings.drawing_origin.into_iter().all(f64::is_finite));
            assert!(settings.paper_scale_factor > 0.0 && settings.drawing_scale > 0.0);
            assert!(settings.drawing_scale.is_finite());
        }
        if let Ok(unit) = doc.text_size_unit() {
            assert_eq!(unit.selector_bytes, data[0x9ac..0x9ae]);
            assert!(matches!(unit.raw_selector, 0 | 1));
        }
        if let Ok(code) = doc.origin_units_code() {
            assert_eq!(code.selector_bytes, data[0x150..0x154]);
            assert_eq!(code.raw_code, i32::from_le_bytes(code.selector_bytes));
        }
        if let Ok(context) = doc.coordinate_measurement_context() {
            assert_eq!(context.paper_scale_bytes, data[0x178..0x180]);
            assert_eq!(context.annotation_ratio_bytes, data[0x278..0x280]);
            for raw in [0.0, 1.0, 1000.0, f64::MAX] {
                for result in [
                    context.length_on_paper_mm(raw),
                    context.length_at_current_annotation_mm(raw),
                ] {
                    if let Ok(length) = result {
                        assert!(length.is_finite() && length >= 0.0);
                    }
                }
            }
        }
        if let Ok(settings) = doc.print_settings() {
            for source in settings.fields() {
                let at = source.payload_offset as usize;
                assert_eq!(source.raw_bytes, data[at..at + source.raw_bytes.len()]);
            }
        }
        if let Ok(styles) = RawStyleTables::parse(&doc) {
            assert_eq!(
                (
                    styles.layers.len(),
                    styles.colors.len(),
                    styles.line_types.len(),
                    styles.widths.len()
                ),
                (257, 256, 32, 16)
            );
            let start = styles.shared_suffix_offset as usize;
            assert_eq!(
                styles.shared_suffix_bytes,
                data[start..start + styles.shared_suffix_bytes.len()]
            );
            for row in styles.layers {
                let start = row.payload_offset as usize;
                assert_eq!(row.raw_bytes, data[start..start + row.raw_bytes.len()]);
            }
        }
        if let Ok(library) = doc.block_library_with_limit(16) {
            for definition in library.definitions() {
                assert!(definition.record_end <= doc.header().payload_end);
                let _ = library.planar_line_records_with_limit(definition.index, 32);
            }
            for reference in doc.package_references().flatten() {
                if reference.record_index < 8 {
                    if let Ok(lines) = library.effective_line_styles(reference.record_index, 32) {
                        assert!(lines.len() <= 32);
                        for line in lines {
                            check_styles(&line.styles, data);
                        }
                    }
                }
                if let Ok(lines) =
                    library.placed_line_records_with_limit(reference.record_index, 32)
                {
                    for line in lines {
                        assert!(line.start.into_iter().chain(line.end).all(f64::is_finite));
                        assert_eq!(line.coordinate_space(), "package_parent");
                        assert_eq!(
                            library
                                .record(line.definition_index, line.source.record_index)
                                .unwrap()
                                .payload_offset,
                            line.source.payload_offset
                        );
                    }
                }
            }
        }
        assert!(doc.record(u64::MAX).is_none());
    }
});
