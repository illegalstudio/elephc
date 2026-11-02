---
id: runbook-take-the-unit-test-baseline-by-running-the-prebu-343704cb
type: runbook
title: "Take the unit-test baseline by running the prebuilt test binary, not cargo test"
description: "cargo test gets SIGKILLed on a hanging test and blocks on the global registry lock a separate CARGO_TARGET_DIR does not avoid; the built binary with --skip answers in seconds and gives an exact failure diff"
created: 2026-09-20
sources:
  - path: src/codegen/lower_term.rs
    blob: 54fbbca9f9b2eb2635ccf17ef88fdaaa456967a1
    lines: 437-470
    snip: f05490714ed8
    anchor: "fn find_numbered_label(asm: &str, prefix: &str) -> String {"
---

# Take the unit-test baseline by running the prebuilt test binary, not cargo test

## Fact

`cargo test` is the wrong way to take a unit-test baseline on this machine. Two attempts were
SIGKILLed, both at `codegen_support::abi::tests::basics::test_emit_frame_prologue_rejects_undersized_frame`
("has been running for over 60 seconds"), and a third sat at 0% CPU for twenty minutes waiting
on the global cargo registry lock that another session held -- a separate `CARGO_TARGET_DIR`
does NOT avoid that lock.

The test binary cargo already built is enough, and it answers in seconds:

    ls -t <target>/release/deps | grep -E '^elephc-[0-9a-f]+$'          # newest is the lib suite
    <target>/release/deps/elephc-<hash> --skip test_emit_frame_prologue_rejects_undersized_frame \
      | grep -E '^test .* FAILED$' | sed 's/^test //; s/ \.\.\. FAILED//' | sort > failures.txt

Do it for `scratchpad/headbase-target` (the exported HEAD tree) and for `target` (the working
tree), then `comm -13 baseline.txt mine.txt`. That names exactly the tests the working tree adds,
and `comm -23` names the ones it fixes.

Measured on this branch: **25 failures at HEAD**, 31 in the working tree. Of the 6 extra, five
sit in files the branch had already modified before the session started and that the session
never touched -- `crates/elephc-builtin-contract/src/catalog_*.rs` (7 files) and
`src/codegen_support/runtime/**` (36 modified, 6 untracked). The sixth pair
(`codegen::lower_term::tests::{cond_br,switch}_arguments_emit_edge_copy_stubs`) WAS the
session's, and the diff is what made that attributable.

That pair was a stale assertion, not a miscompile, and checking which mattered: the test's
`find_numbered_label` required `prefix_<digits>`, while the labels now read
`.L_eir_main_cond_then_args_<module-hex>_<n>` -- a module-uniquing infix added so two modules
cannot give one helper two meanings. The stubs were still emitted AND still branched to; only
the matcher was wrong. Take the LAST underscore-separated component as the counter.

## Why

Three cargo runs were lost to this before the binary trick; and the diff is the only thing that turned 32 failures into 'five are the branch's, two were mine and are stale assertions'.
