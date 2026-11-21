"""DRA-CAD inspection foundation backed by a standalone Rust core."""

from importlib.metadata import version

from . import best_effort, raw
from ._core import (
    DracadError,
    DracadIoError,
    InvalidFormatError,
    LimitExceededError,
    UnsupportedFeatureError,
    UnsupportedFormatError,
)
from .basic2d import ArcEntity, Basic2DProfile, CircleEntity, TextEntity, TextLimits
from .document import (
    Document,
    Entity,
    EntityQuery,
    IncompleteReadError,
    LineEntity,
    OpaqueRegion,
    ReadDiagnostic,
    ReadLimits,
    ReadOptions,
    ReadPolicy,
    ReadReport,
    ReadScope,
    ReadStatus,
    SourceRef,
    read,
    readfile,
)
from .options import ParseLimits

__version__ = version("ezdracad")

__all__ = [
    "DracadError",
    "DracadIoError",
    "InvalidFormatError",
    "LimitExceededError",
    "ParseLimits",
    "UnsupportedFeatureError",
    "UnsupportedFormatError",
    "__version__",
    "best_effort",
    "raw",
    "Document",
    "ArcEntity",
    "Basic2DProfile",
    "CircleEntity",
    "Entity",
    "EntityQuery",
    "IncompleteReadError",
    "LineEntity",
    "OpaqueRegion",
    "ReadDiagnostic",
    "ReadLimits",
    "ReadOptions",
    "ReadPolicy",
    "ReadReport",
    "ReadScope",
    "ReadStatus",
    "SourceRef",
    "TextEntity",
    "TextLimits",
    "read",
    "readfile",
]
