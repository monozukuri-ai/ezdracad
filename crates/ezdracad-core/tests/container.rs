use std::io::{Cursor, Write};

use ezdracad_core::{inspect_cfb, CfbEntryKind, DracadError, ParseLimits};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/minimal.cfb");

#[test]
fn inspects_independently_assembled_cfb() {
    let info = inspect_cfb(FIXTURE, &ParseLimits::default()).unwrap();
    assert_eq!(info.version, 3);
    assert_eq!(info.root_clsid, "00000000-0000-0000-0000-000000000000");
    assert_eq!(info.total_stream_size, 11);
    assert_eq!(info.entries.len(), 2);
    assert_eq!(info.entries[0].path, "/Example");
    assert_eq!(info.entries[0].kind, CfbEntryKind::Storage);
    assert_eq!(info.entries[0].size, 0);
    assert_eq!(info.entries[1].path, "/Example/Note");
    assert_eq!(info.entries[1].kind, CfbEntryKind::Stream);
    assert_eq!(info.entries[1].size, 11);
}

#[test]
fn accepts_backend_v4_fixture_without_drawing_identification() {
    let mut compound =
        cfb::CompoundFile::create_with_version(cfb::Version::V4, Cursor::new(Vec::new())).unwrap();
    compound
        .create_stream("/arbitrary")
        .unwrap()
        .write_all(b"data")
        .unwrap();
    let bytes = compound.into_inner().into_inner();
    let info = inspect_cfb(&bytes, &ParseLimits::default()).unwrap();
    assert_eq!(info.version, 4);
    assert_eq!(info.entries[0].path, "/arbitrary");
    assert_eq!(info.total_stream_size, 4);
}

#[test]
fn rejects_invalid_and_truncated_containers() {
    for bytes in [&b"not cfb"[..], &FIXTURE[..512], &FIXTURE[..1024]] {
        assert!(matches!(
            inspect_cfb(bytes, &ParseLimits::default()),
            Err(DracadError::InvalidFormat(_))
        ));
    }
    let mut cyclic = FIXTURE.to_vec();
    // Make the directory sector point to itself in the FAT.
    cyclic[1024..1028].copy_from_slice(&0_u32.to_le_bytes());
    assert!(matches!(
        inspect_cfb(&cyclic, &ParseLimits::default()),
        Err(DracadError::InvalidFormat(_))
    ));
}

#[test]
fn applies_file_limit_before_backend_parsing() {
    let limits = ParseLimits {
        max_file_size: 0,
        ..ParseLimits::default()
    };
    assert!(matches!(
        inspect_cfb(b"x", &limits),
        Err(DracadError::LimitExceeded {
            resource: "file_size",
            limit: 0,
            actual: 1
        })
    ));
}

#[test]
fn inventory_limits_include_storages_and_declared_stream_sizes() {
    let defaults = ParseLimits::default();
    let cases = [
        (
            ParseLimits {
                max_cfb_entries: 1,
                ..defaults
            },
            "cfb_entries",
            1,
            2,
        ),
        (
            ParseLimits {
                max_stream_size: 10,
                ..defaults
            },
            "stream_size",
            10,
            11,
        ),
        (
            ParseLimits {
                max_total_stream_size: 10,
                ..defaults
            },
            "total_stream_size",
            10,
            11,
        ),
    ];
    for (limits, name, expected_limit, expected_actual) in cases {
        match inspect_cfb(FIXTURE, &limits).unwrap_err() {
            DracadError::LimitExceeded {
                resource,
                limit,
                actual,
            } => {
                assert_eq!(
                    (resource, limit, actual),
                    (name, expected_limit, expected_actual)
                );
            }
            error => panic!("unexpected error: {error}"),
        }
    }
    let exact = ParseLimits {
        max_file_size: FIXTURE.len() as u64,
        max_cfb_entries: 2,
        max_stream_size: 11,
        max_total_stream_size: 11,
        ..ParseLimits::default()
    };
    inspect_cfb(FIXTURE, &exact).unwrap();
}
