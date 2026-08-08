#!/usr/bin/env python3
"""Guard against drift between the PHP prelude's `extern "elephc_image"` block and
the staticlib's exported symbols.

The prelude (`src/image_prelude.rs`) declares one `function elephc_*(...)` per
bridge entry point inside its `extern "elephc_image"` block; the crate exports a
matching `#[no_mangle] pub extern "C" fn elephc_*` for each. The two lists are
kept in sync by hand, so a declared symbol with no export would only surface as a
link error in a compiled PHP program. This check fails fast instead.

Run from the repo root (CI and pre-commit):

    python3 crates/elephc-image/tools/check_extern_exports.py

Exit status: 0 when every declared extern has an export, 1 on any mismatch.
"""
import os
import re
import sys
import glob

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
PRELUDE = os.path.join(ROOT, "src", "image_prelude.rs")
# Recursive so submodule directories (e.g. `src/cairo/`) are scanned too.
CRATE_SRC = os.path.join(ROOT, "crates", "elephc-image", "src", "**", "*.rs")


def declared_externs():
    """Names the prelude declares as bridge entry points.

    The prelude is `synthetic_class` builder calls, not PHP text: one
    `extern_fn("elephc_x", "elephc_image")` per entry point, where the PHP form had a
    `function elephc_x(...)` inside an `extern "elephc_image" {}` block. The ABI it names is
    part of the pattern, so a declaration bound to some other library is not counted here.
    """
    src = open(PRELUDE, encoding="utf-8").read()
    return set(re.findall(r'extern_fn\("(elephc_\w+)", "elephc_image"\)', src))


def exported_symbols():
    """Names exported as `#[no_mangle] pub [unsafe] extern "C" fn elephc_*`."""
    out = set()
    for path in glob.glob(CRATE_SRC, recursive=True):
        src = open(path, encoding="utf-8").read()
        for m in re.finditer(
            r'#\[no_mangle\]\s*pub (?:unsafe )?extern "C" fn (elephc_\w+)', src
        ):
            out.add(m.group(1))
    return out


def main():
    declared = declared_externs()
    exported = exported_symbols()
    missing = sorted(declared - exported)  # declared in prelude, no crate export -> link error
    extra = sorted(exported - declared)    # exported but never declared -> dead symbol

    if missing:
        print("ERROR: prelude declares externs with no matching crate export:", file=sys.stderr)
        for name in missing:
            print(f"  - {name}", file=sys.stderr)
    if extra:
        print("WARNING: crate exports symbols not declared in the prelude (dead):", file=sys.stderr)
        for name in extra:
            print(f"  - {name}", file=sys.stderr)

    print(f"prelude externs: {len(declared)}, crate exports: {len(exported)}, "
          f"missing: {len(missing)}, extra: {len(extra)}")
    sys.exit(1 if missing else 0)


if __name__ == "__main__":
    main()
