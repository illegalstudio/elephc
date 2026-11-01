---
id: gotcha-a-compiled-program-s-interpreter-comes-from-libe-d0688a4d
type: gotcha
title: "A compiled program's interpreter comes from libelephc_magician.a, which --bin elephc may not rebuild"
description: "After changing Magician or the builtin catalog, build ALL release targets before testing a compiled program: a stale static library silently keeps the OLD interpreter behaviour"
created: 2026-09-17
verified_by: "MB_CASE_UPPER resolved on the compiled path but stayed undefined inside eval() until cargo build --release refreshed target/release/libelephc_magician.a"
sources:
  - path: crates/elephc-magician/src/interpreter/constant_eval.rs
    blob: a4e6e6c66e6f19ef503c72d4912ec98565ff645a
  - path: crates/elephc-builtin-contract/src/catalog_constants.rs
    blob: 0239907dfe6ebb762596213944b30ec78e58dcfe
---

# A compiled program's interpreter comes from libelephc_magician.a, which --bin elephc may not rebuild

## Fact

`elephc` LINKS `target/release/libelephc_magician.a` into every program it compiles, so a compiled
binary's INTERPRETER is whatever that archive last contained — not whatever the `elephc` binary
was built from.

`cargo build --release --bin elephc` builds the compiler and the rlibs it depends on. It does not
reliably refresh the staticlib crate-type, so an interpreter or catalog change can be live on the
COMPILED path and stale on the EVAL path at the same time. That split is the tell:

    echo 'eval("echo MB_CASE_UPPER;");' ...
    compiled: 0            <- the new catalog IS in the elephc binary
    eval:     undefined    <- the old catalog is still in the .a

    ls -la target/release/libelephc_magician.a    # older than the edit == stale

Run `cargo build --release` (all targets) after touching `crates/elephc-magician/**` or
`crates/elephc-builtin-contract/**`, then recompile the PHP program. Checking the archive's
timestamp against the edit is the one-line confirmation.

## Why

It costs a full wrong diagnosis: the symptom looks exactly like the fix not working
