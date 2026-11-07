"""Build distributable wheels with portable Rust source paths."""

import argparse
import os
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("dist"))
    parser.add_argument(
        "--sdist", action="store_true", help="also build a source distribution"
    )
    args = parser.parse_args()
    env = dict(os.environ)
    encoded = env.pop("CARGO_ENCODED_RUSTFLAGS", "")
    flags = encoded.split("\x1f") if encoded else shlex.split(env.pop("RUSTFLAGS", ""))
    flags.extend(
        [
            f"--remap-path-prefix={Path.home()}=/build-cache",
            f"--remap-path-prefix={ROOT}=.",
        ]
    )
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    out = str(args.out.resolve())
    subprocess.run(
        [
            sys.executable,
            "-m",
            "maturin",
            "build",
            "--release",
            "--locked",
            "--out",
            out,
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )
    if args.sdist:
        subprocess.run(
            [sys.executable, "-m", "maturin", "sdist", "--out", out],
            cwd=ROOT,
            env=env,
            check=True,
        )


if __name__ == "__main__":
    main()
