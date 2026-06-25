#!/usr/bin/env python3
"""Extract builtins from the Elephc source tree and render Markdown docs.

Run from the repo root:

    python3 scripts/docs/extract_builtins.py            # parse + write JSON
    python3 scripts/docs/extract_builtins.py --render   # also render pages
    python3 scripts/docs/extract_builtins.py --force    # overwrite hand-written pages

Output:
- scripts/docs/builtin_registry.json         (the canonical data)
- docs/php/builtins/<name>.md                (user reference)
- docs/php/builtins/_<area>.md               (per-area index)
- docs/internals/builtins/<name>.md          (compiler internals)
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

THIS = Path(__file__).resolve()
HERE = THIS.parent
sys.path.insert(0, str(HERE))

import elephc_builtins.extract as builtins_extract  # noqa: E402
import elephc_builtins.render as builtins_render  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=HERE.parents[1],
        help="Path to the Elephc repo root.",
    )
    parser.add_argument(
        "--render",
        action="store_true",
        help="Render Markdown pages after extracting.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing hand-written pages.",
    )
    parser.add_argument(
        "--out-json",
        type=Path,
        default=None,
    )
    args = parser.parse_args()

    repo = args.repo_root.resolve()
    out_json = (args.out_json or repo / "scripts" / "docs" / "builtin_registry.json").resolve()

    # 1. Extract
    rc = builtins_extract.main_with(
        repo_root=repo,
        out=out_json,
    )
    if rc != 0:
        return rc

    # 2. Render (optional)
    if args.render:
        return builtins_render.main_with(
            repo_root=repo,
            registry=out_json,
            force=args.force,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
