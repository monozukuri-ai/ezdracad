"""MPZ/MPX/MPS inspection, raw scanning and read-only partial extraction."""

import argparse
import json
import sys
from collections.abc import Sequence
from dataclasses import asdict, fields

from . import (
    DracadError,
    LimitExceededError,
    ParseLimits,
    UnsupportedFeatureError,
    UnsupportedFormatError,
    __version__,
    best_effort,
    raw,
    readfile,
)
from .basic2d import Basic2DProfile, TextLimits
from .document import ReadLimits, ReadScope, ReadStatus
from .qualification import Line2DProfile


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="DRA-CAD MPZ/MPX/MPS inspection and partial reading"
    )
    parser.add_argument(
        "--version", action="version", version=f"ezdracad {__version__}"
    )
    subparsers = parser.add_subparsers(dest="command")
    inspection = subparsers.add_parser(
        "inspect", help="inspect MPZ/MPX/MPS input metadata"
    )
    scanning = subparsers.add_parser(
        "scan", help="scan qualified MPZ/MPX/MPS raw records"
    )
    extraction = subparsers.add_parser(
        "extract", help="extract qualified entities with a read report (JSON)"
    )
    extraction.add_argument(
        "--entities", choices=["LINE", "CIRCLE", "ARC", "TEXT", "*"], default="LINE"
    )
    policies = extraction.add_mutually_exclusive_group()
    policies.add_argument("--policy", choices=["strict", "partial"], default="strict")
    policies.add_argument(
        "--partial", action="store_const", const="partial", dest="policy"
    )
    extraction.add_argument(
        "--scope", choices=[s.value for s in ReadScope], default="drawing"
    )
    extraction.add_argument(
        "--line-profile",
        choices=[p.value for p in Line2DProfile],
        default=Line2DProfile.MARKER25_PLANAR_STORED_XY_V1.value,
    )
    extraction.add_argument(
        "--basic-profile",
        choices=[p.value for p in Basic2DProfile],
        help="explicit root CIRCLE/ARC/TEXT decoder profile",
    )
    for option in fields(TextLimits):
        extraction.add_argument(
            "--" + option.name.replace("_", "-"), type=int, metavar="N"
        )
    for option in fields(ReadLimits):
        extraction.add_argument(
            "--" + option.name.replace("_", "-"), type=int, metavar="N"
        )
    reading = subparsers.add_parser(
        "best-effort",
        help="read every recognized object and report the rest",
    )
    reading.add_argument(
        "--entities",
        action="store_true",
        help="include entities and block definitions in the JSON output",
    )
    for option in fields(best_effort.BestEffortLimits):
        reading.add_argument(
            "--" + option.name.replace("_", "-"), type=int, metavar="N"
        )
    for command in (inspection, scanning, extraction, reading):
        command.add_argument("path")
        command.add_argument("--json", action="store_true", help="print JSON report")
        for option in fields(ParseLimits):
            command.add_argument(
                "--" + option.name.replace("_", "-"), type=int, metavar="N"
            )
    scanning.add_argument(
        "--records", action="store_true", help="include one metadata page"
    )
    scanning.add_argument(
        "--line-candidates",
        action="store_true",
        help="include candidate XY in the raw-record window",
    )
    scanning.add_argument("--start", type=int, default=0, help="raw-record start index")
    scanning.add_argument(
        "--count", type=int, default=1024, help="raw-record window size, maximum 4096"
    )
    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help()
        return 0
    if args.command == "scan" and (
        args.start < 0 or not 0 <= args.count <= raw.MAX_RECORDS_PER_PAGE
    ):
        parser.error("--start must be nonnegative; --count must be in 0..4096")
    try:
        if args.command == "extract":
            text_limits = TextLimits(
                **{
                    option.name: getattr(args, option.name)
                    for option in fields(TextLimits)
                    if getattr(args, option.name) is not None
                }
            )
            read_limits = ReadLimits(
                **{
                    option.name: getattr(args, option.name)
                    for option in fields(ReadLimits)
                    if getattr(args, option.name) is not None
                }
            )
        elif args.command == "best-effort":
            read_limits = best_effort.BestEffortLimits(
                **{
                    option.name: getattr(args, option.name)
                    for option in fields(best_effort.BestEffortLimits)
                    if getattr(args, option.name) is not None
                }
            )
        else:
            read_limits = None
        limits = ParseLimits(
            **{
                option.name: getattr(args, option.name)
                for option in fields(ParseLimits)
                if getattr(args, option.name) is not None
            }
        )
    except (TypeError, ValueError) as error:
        parser.error(str(error))
    try:
        if args.command == "extract":
            doc = readfile(
                args.path,
                policy=args.policy,
                scope=args.scope,
                line_profile=args.line_profile,
                basic_profile=args.basic_profile,
                text_limits=text_limits,
                limits=limits,
                read_limits=read_limits,
            )
            result = doc.to_dict(include_entities=False)
            result["entities"] = [
                entity.to_dict() for entity in doc.query(args.entities)
            ]
            result["query"] = {
                "entity_type": args.entities,
                "returned_entity_count": len(result["entities"]),
                "decoded_entity_count": doc.report.entity_count,
            }
            print(json.dumps(result, sort_keys=True, allow_nan=False))
            return 0 if doc.report.status == ReadStatus.COMPLETE else 2
        if args.command == "best-effort":
            drawing = best_effort.readfile(
                args.path, limits=limits, read_limits=read_limits
            )
            report = drawing.report
            if args.json:
                print(
                    json.dumps(
                        drawing.to_dict(include_entities=args.entities),
                        sort_keys=True,
                        allow_nan=False,
                    )
                )
            else:
                print(f"Format: {drawing.format}, raw marker {drawing.marker}")
                for name, count in sorted(report.decoded_counts.items()):
                    print(f"{name}: {count}")
                for tag, count in sorted(report.unsupported_counts.items()):
                    print(f"not decoded, object kind {tag}: {count}")
                for tag, count in sorted(report.failed_counts.items()):
                    print(f"skipped, object kind {tag}: {count}")
                if report.orphan_records or report.clamped_spans:
                    print(
                        f"stray continuation records: {report.orphan_records}; "
                        f"objects cut at their parent: {report.clamped_spans}"
                    )
            return 2 if report.lossy else 0
        if args.command == "scan":
            scan = raw.scanfile(args.path, limits=limits)
            if args.start > scan.header.record_count:
                parser.error("--start exceeds record count")
            report = scan.to_dict()
            if args.records or args.line_candidates:
                report["page"] = {
                    "record_start": args.start,
                    "record_count_requested": args.count,
                    "record_end": min(
                        args.start + args.count, scan.header.record_count
                    ),
                }
            if args.records:
                report["records"] = [
                    asdict(item) for item in scan.records(args.start, args.count)
                ]
            if args.line_candidates:
                report["line_candidates"] = [
                    asdict(item)
                    for item in scan.line_candidates(args.start, args.count)
                ]
            if args.json:
                print(json.dumps(report, sort_keys=True, allow_nan=False))
            else:
                print(f"Profile: {scan.header.profile}")
                print(
                    f"Records: {scan.header.record_count} (including terminal), opaque: {scan.opaque_record_count}"
                )
                print(
                    f"Provisional XY candidates: {scan.line_candidate_count}; units/transforms unresolved"
                )
                if args.records or args.line_candidates:
                    print(
                        json.dumps(
                            {
                                key: report[key]
                                for key in ("page", "records", "line_candidates")
                                if key in report
                            }
                        )
                    )
            return 0
        info = raw.inspectfile(args.path, limits=limits)
    except DracadError as error:
        detail = {
            "schema_version": 1,
            "error": type(error).__name__,
            "message": str(error),
        }
        if hasattr(error, "report"):
            detail["report"] = error.report.to_dict()
        if isinstance(error, LimitExceededError):
            detail.update(
                resource=error.resource, limit=error.limit, actual=error.actual
            )
        if args.json or args.command == "extract":
            print(json.dumps(detail), file=sys.stderr)
        else:
            print(f"error: {error}", file=sys.stderr)
        if isinstance(error, LimitExceededError):
            return 3
        return (
            2
            if isinstance(error, (UnsupportedFormatError, UnsupportedFeatureError))
            else 1
        )
    if args.json:
        print(json.dumps(info.to_dict(), sort_keys=True))
    else:
        print(f"Format: {info.format} (input inspection only)")
        if info.payload is not None:
            if info.payload.offset_domain == "file":
                print(f"MPX file payload: {info.payload.size} bytes")
            elif info.payload.offset_domain == "decoded_payload":
                print(f"Decoded MPS payload: {info.payload.size} bytes")
            else:
                print(
                    f"Contents: {info.payload.stream_path}, {info.payload.size} bytes"
                )
            print(f"SHA-256: {info.payload.sha256}")
            print(f"Raw marker: {info.payload.marker_raw} (uninterpreted)")
        for diagnostic in info.diagnostics:
            print(f"{diagnostic.code}: {diagnostic.message}")
    return 0 if info.format in ("mpz", "mpx", "mps") else 2
