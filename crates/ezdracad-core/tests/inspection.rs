use std::io::{Cursor, Write};

use ezdracad_core::{
    inspect, inspect_cfb, ContainerInfo, DracadError, DracadFormat, ParseLimits, DRACAD_CLSID,
};

const GENERIC: &[u8] = include_bytes!("../../../tests/fixtures/minimal.cfb");
const ENVELOPE: &[u8] = include_bytes!("../../../tests/fixtures/mpz-envelope.cfb");
const CONTENTS: &[u8] = include_bytes!("../../../tests/fixtures/mpz-contents.bin");

fn envelope(stream: &str, contents: &[u8], extra: bool) -> Vec<u8> {
    let mut file = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    file.set_storage_clsid("/", uuid::Uuid::parse_str(DRACAD_CLSID).unwrap())
        .unwrap();
    if stream.contains("/Nested/") {
        file.create_storage("/Nested").unwrap();
    }
    file.create_stream(stream)
        .unwrap()
        .write_all(contents)
        .unwrap();
    if extra {
        file.create_storage("/Embedded").unwrap();
        file.create_stream("/Embedded/Unknown")
            .unwrap()
            .write_all(b"opaque bytes")
            .unwrap();
    }
    file.into_inner().into_inner()
}

fn inspect_default(bytes: &[u8]) -> ContainerInfo {
    inspect(bytes, &ParseLimits::default()).unwrap()
}

#[test]
fn independent_envelope_extracts_exact_bytes_and_raw_marker() {
    let info = inspect_default(ENVELOPE);
    assert_eq!(info.format, DracadFormat::Mpz);
    assert_eq!(info.file_size, ENVELOPE.len() as u64);
    assert_eq!(info.contents().unwrap(), CONTENTS);
    let payload = info.payload.unwrap();
    assert_eq!(payload.stream_path.as_deref(), Some("/Contents"));
    assert_eq!(payload.size, CONTENTS.len() as u64);
    assert_eq!(payload.marker_raw, 0x3456);
    assert!(info
        .diagnostics
        .iter()
        .any(|item| item.code == "geometry_not_decoded"));
}

#[test]
fn generic_cfb_and_clsid_alone_do_not_identify_a_drawing() {
    assert_eq!(inspect_default(GENERIC).format, DracadFormat::UnknownCfb);
    let mut without_clsid = ENVELOPE.to_vec();
    without_clsid[592..608].fill(0);
    assert_eq!(
        inspect_default(&without_clsid).format,
        DracadFormat::UnknownCfb
    );
    let mut clsid_only = GENERIC.to_vec();
    clsid_only[592..608].copy_from_slice(&ENVELOPE[592..608]);
    let info = inspect_default(&clsid_only);
    assert_eq!(info.format, DracadFormat::UnknownCfb);
    assert!(matches!(
        info.contents(),
        Err(DracadError::UnsupportedFormat(_))
    ));
}

#[test]
fn stream_names_are_root_only_and_case_insensitive() {
    assert_eq!(
        inspect_default(&envelope("/contents", CONTENTS, false)).format,
        DracadFormat::Mpz
    );
    assert_eq!(
        inspect_default(&envelope("/Nested/Contents", CONTENTS, false)).format,
        DracadFormat::UnknownCfb
    );
}

#[test]
fn mps_is_a_candidate_with_no_decoded_payload() {
    let info = inspect_default(&envelope("/SecureContents", b"opaque", false));
    assert_eq!(info.format, DracadFormat::MpsCandidate);
    assert!(info.payload.is_none());
    assert!(matches!(
        info.contents(),
        Err(DracadError::UnsupportedFormat(_))
    ));
}

#[test]
fn preserves_additional_entries_without_interpreting_them() {
    let info = inspect_default(&envelope("/Contents", CONTENTS, true));
    assert_eq!(info.contents().unwrap(), CONTENTS);
    assert_eq!(info.cfb.as_ref().unwrap().entries.len(), 3);
    assert!(info
        .diagnostics
        .iter()
        .any(|item| item.code == "additional_cfb_entries"));
}

#[test]
fn reads_regular_fat_contents_as_well_as_mini_streams() {
    let mut contents = CONTENTS.to_vec();
    contents.resize(9000, 0x55);
    assert_eq!(
        inspect_default(&envelope("/Contents", &contents, false))
            .contents()
            .unwrap(),
        contents
    );
}

#[test]
fn accepts_a_positive_difat_chain() {
    let mut contents = CONTENTS.to_vec();
    contents.resize(8 * 1024 * 1024, 0x55);
    // Default cfb::create uses v4; use v3 to exceed its 109 header FAT slots.
    let mut file =
        cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new())).unwrap();
    file.set_storage_clsid("/", uuid::Uuid::parse_str(DRACAD_CLSID).unwrap())
        .unwrap();
    file.create_stream("/Contents")
        .unwrap()
        .write_all(&contents)
        .unwrap();
    let bytes = file.into_inner().into_inner();
    assert!(u32::from_le_bytes(bytes[72..76].try_into().unwrap()) > 0);
    assert_eq!(inspect_default(&bytes).contents().unwrap(), contents);
}

#[test]
fn rejects_bad_identifier_truncation_ambiguous_streams_and_broken_chains() {
    let mut bad_magic = ENVELOPE.to_vec();
    bad_magic[1536] = 0;
    let mut bad_padding = ENVELOPE.to_vec();
    bad_padding[1536 + 20] = 1;
    let mut cyclic = ENVELOPE.to_vec();
    cyclic[2048..2052].copy_from_slice(&0_u32.to_le_bytes());
    let mut file = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    file.set_storage_clsid("/", uuid::Uuid::parse_str(DRACAD_CLSID).unwrap())
        .unwrap();
    file.create_stream("/Contents")
        .unwrap()
        .write_all(CONTENTS)
        .unwrap();
    file.create_stream("/SecureContents")
        .unwrap()
        .write_all(b"opaque")
        .unwrap();
    let ambiguous = file.into_inner().into_inner();
    let short = envelope("/Contents", &CONTENTS[..35], false);
    for bytes in [
        &bad_magic[..],
        &bad_padding[..],
        &cyclic[..],
        &ENVELOPE[..ENVELOPE.len() - 1],
        &ambiguous[..],
        &short[..],
    ] {
        assert!(matches!(
            inspect(bytes, &ParseLimits::default()),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    assert!(matches!(
        inspect(CONTENTS, &ParseLimits::default()),
        Err(DracadError::UnsupportedFormat(_))
    ));
}

#[test]
fn preflight_limits_apply_before_backend_metadata_is_allocated() {
    let defaults = ParseLimits::default();
    let limits = [
        (
            ParseLimits {
                max_cfb_metadata_size: 512,
                ..defaults
            },
            "cfb_metadata_size",
        ),
        (
            ParseLimits {
                max_cfb_directory_slots: 3,
                ..defaults
            },
            "cfb_directory_slots",
        ),
        (
            ParseLimits {
                max_cfb_depth: 1,
                ..defaults
            },
            "cfb_depth",
        ),
    ];
    for (limits, name) in limits {
        assert!(
            matches!(inspect_cfb(GENERIC, &limits), Err(DracadError::LimitExceeded { resource, .. }) if resource == name)
        );
    }
    let info = inspect_cfb(GENERIC, &defaults).unwrap();
    assert_eq!(
        (info.metadata_size, info.directory_slots, info.depth),
        (2048, 4, 2)
    );
}

#[test]
fn preflight_rejects_invalid_metadata_graphs_and_huge_counts() {
    let mut huge_fat = GENERIC.to_vec();
    huge_fat[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut directory_cycle = GENERIC.to_vec();
    directory_cycle[640 + 76..640 + 80].copy_from_slice(&0_u32.to_le_bytes());
    let mut mini_fat_cycle = GENERIC.to_vec();
    mini_fat_cycle[1024 + 12..1024 + 16].copy_from_slice(&3_u32.to_le_bytes());
    for data in [huge_fat, directory_cycle, mini_fat_cycle] {
        assert!(matches!(
            inspect_cfb(&data, &ParseLimits::default()),
            Err(DracadError::InvalidFormat(_))
        ));
    }
}
