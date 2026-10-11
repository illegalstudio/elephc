---
id: gotcha-callable-validation-must-honor-target-registrati-603f8c0a
type: gotcha
title: "Callable validation must honor target registration before signatures"
description: "iterator_apply must not recover Unix-only builtin signatures on Windows through raw catalog lookup"
tags: [windows, callbacks]
created: 2026-10-11
verified_by: "cargo +1.98 test -p elephc --lib --features curl windows"
stale_when: "Static callback resolution or target builtin availability policy changes"
sources:
  - path: src/types/checker/builtins/callables.rs
    blob: 5f7f78c53d1d3a0e0e2de5ef189b3d55720bd5ed
    lines: 1105-1128
    snip: 3975dfb2d8fc
    anchor: "if crate::name_resolver::is_builtin_function(cb_name.as_str()) {"
  - path: src/types/result.rs
    blob: 965e71be436ef265d4ae42ef1c66c8774beca15f
    lines: 369-412
    snip: 882e9a387b32
    anchor: "fn windows_rejects_unavailable_lchown_callable_surfaces() {"
---

# Callable validation must honor target registration before signatures

## Fact

The raw builtin catalog is a physical inventory, not proof that PHP registers a name on the selected target. Static callback validation must check builtin_is_available_for_target before reading or validating first-class builtin signatures and requirements. This preserves Undefined function diagnostics for absent Windows lchown/lchgrp callbacks rather than producing an argument-type error. Existing user functions retain precedence before builtin lookup. Fresh Windows-focused compiler tests pass 176, including unavailable callable surfaces and user-defined homonyms.
