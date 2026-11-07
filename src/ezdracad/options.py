"""Parser resource limits. Defaults come from the Rust core."""

from dataclasses import dataclass, fields

from . import _core


@dataclass(frozen=True)
class ParseLimits:
    """Byte/count limits; CFB entries exclude the root storage.

    Metadata bytes, directory slots, entry count and depth are checked before
    backend parsing. Metadata bytes are an on-disk budget, not a heap quota.
    Zero limits are permitted.
    """

    max_file_size: int = _core.DEFAULT_MAX_FILE_SIZE
    max_cfb_entries: int = _core.DEFAULT_MAX_CFB_ENTRIES
    max_stream_size: int = _core.DEFAULT_MAX_STREAM_SIZE
    max_total_stream_size: int = _core.DEFAULT_MAX_TOTAL_STREAM_SIZE
    max_cfb_metadata_size: int = _core.DEFAULT_MAX_CFB_METADATA_SIZE
    max_cfb_directory_slots: int = _core.DEFAULT_MAX_CFB_DIRECTORY_SLOTS
    max_cfb_depth: int = _core.DEFAULT_MAX_CFB_DEPTH
    max_records: int = _core.DEFAULT_MAX_RECORDS
    max_decompressed_size: int = _core.DEFAULT_MAX_DECOMPRESSED_SIZE

    def __post_init__(self) -> None:
        for field in fields(self):
            value = getattr(self, field.name)
            if not isinstance(value, int) or isinstance(value, bool):
                raise TypeError(f"{field.name} must be an integer")
            if not 0 <= value <= (1 << 64) - 1:
                raise ValueError(f"{field.name} must fit an unsigned 64-bit integer")

    def _values(self) -> tuple[int, int, int, int, int, int, int, int, int]:
        return (
            self.max_file_size,
            self.max_cfb_entries,
            self.max_stream_size,
            self.max_total_stream_size,
            self.max_cfb_metadata_size,
            self.max_cfb_directory_slots,
            self.max_cfb_depth,
            self.max_records,
            self.max_decompressed_size,
        )
