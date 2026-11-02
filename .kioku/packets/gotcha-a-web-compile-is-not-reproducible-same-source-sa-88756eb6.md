---
id: gotcha-a-web-compile-is-not-reproducible-same-source-sa-88756eb6
type: gotcha
title: "A --web compile is not reproducible: same source, same flags, different assembly"
description: "One cold compile in four differs; non-web compiles are stable. Found while diffing a failing CLI test"
tags: [codegen, web, determinism]
created: 2026-09-16
verified_by: "scratchpad/determinism.sh: 4 cold --emit-asm compiles identical, 4 cold --web compiles give 2 distinct hashes"
sources:
  - path: src/codegen/mod.rs
    blob: 69052f33fb2f100aae633c2fae43dfccb5383e32
---

# A --web compile is not reproducible: same source, same flags, different assembly

## Fact

    printf "<?php echo 'ok';" > main.php
    # four times, wiping XDG_CACHE_HOME between:
    elephc --emit-asm main.php   -> 4 identical hashes
    elephc --web      main.php   -> 3 identical, 1 different

The difference is a FRAME LAYOUT change in a prelude function (`_fn_setcookie` saved x25/x26 in one
run and not the other, shifting ~2600 lines of offsets). It is not the runtime cache: two COLD
compiles differ too, and a warm one differs from a cold one only because of this.

`--web` reports `Assembler: 8 parallel slices`, so the likely cause is an order dependence between
the parallel assembly slices and the shared temporary-promotion decision (`6350 temporaries
promoted`). A non-web compile of the same source does not trip it.

This is what makes `codegen::cli::test_cli_web_isolation_selects_entry_symbol_at_compile_time`
fail intermittently — it asserts that plain `--web` and `--web --web-isolation=worker` emit
byte-identical assembly, which is a reasonable thing to assert and is exactly what is violated.
Reproduce it from a shell before blaming a source change; it reproduces on a tree with no edits.
