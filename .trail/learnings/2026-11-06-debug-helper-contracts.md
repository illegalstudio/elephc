---
title: Debug helpers are PreludeProvided contracts, not a fifth registry surface
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/spec.rs
  - crates/elephc-builtin-contract/src/registry.rs
  - crates/elephc-builtin-contract/src/support.rs
  - src/debug_prelude.rs
  - src/builtins/parity_tests.rs
  - src/builtins/docs.rs
  - tests/builtin_parity_tests.rs
  - tools/gen_builtins.rs
  - scripts/docs/elephc_builtins/registry.py
  - scripts/docs/elephc_builtins/extract.py
  - scripts/docs/audit_builtins.py
  - docs/php/builtins/debug.md
  - docs/php/builtins/debug/elephc_break.md
  - docs/php/builtins/debug/elephc_dump.md
  - docs/php/builtins/debug/elephc_trace_start.md
  - docs/php/builtins/debug/elephc_trace_end.md
---

## Decided

- The four elephc debug helpers are `BuiltinKind::PreludeProvided` contracts with
  `extension: true`, `internal: false`, `Area::Debug`, and the debug prelude as
  their AOT implementation. They do not get a `builtin!` registry binding,
  because the prelude is the whole implementation.
- `elephc_break()` is injected as a real helper in every `--debug` build and as
  an empty-body no-op stub in normal builds; `elephc_dump()` and the trace helpers
  remain debug-only. Contracts describe the surface; the prelude declaration is
  what makes a name callable, which is why non-debug `elephc_dump()` still reports
  `Undefined function`.
- `elephc_dump()` validates `$to` in PHP before calling the native recorder. An
  invalid destination raises `ValueError` with PHP's argument-position spelling;
  valid destinations are `auto`, `stderr`, `terminal` and `html`. The runtime
  warning remains defensive, not the user-facing path.
- `Area::Debug` is a first-class contract area and docs category. It maps to
  `src/debug_prelude.rs` in `PRELUDE_SOURCES`, `("Debug", "Debug")` in the docs
  registry, and `"debug"` in the exporter area match. The generated pages live at
  `docs/php/builtins/debug/`.
- The builtin-contract counts move by four:
  - contracts 1201 (+ curl feature surface);
  - eval-pending 365;
  - non-registry AOT routes 428 (+ curl surface);
  - eval-unsupported 499.
- The debug prelude is part of `injected_prelude_programs`, so both parity
  directions run: every `PreludeProvided` contract must be declared by a prelude,
  and every PHP-visible prelude declaration must have a contract.

## Tried and ruled out

- Adding `builtin!` homes for the four helpers: the prelude is the implementation,
  and duplicate registry entries would make the AOT route disagree with the
  documented execution model.
- Mapping the contracts to `Area::System` to reuse `version_prelude.rs`: the
  signature/documentation audit would look for `elephc_break` in the version
  prelude and fail (or, worse, point every page at an unrelated line). A dedicated
  area is the only correct source mapping.
- Making `elephc_dump()`/traces callable in normal builds through the catalog:
  the catalog does not inject preludes. The prelude is the only declaration that
  makes a `PreludeProvided` name resolvable, so normal-build calls remain
  undefined unless a stub is injected deliberately (only `elephc_break`).
- Raising `ValueError` from the native event call: the C ABI cannot cheaply
  construct a PHP throwable. The PHP wrapper validates before entering native code.

## Watch out

- The generated docs are canonical with `--features curl`; rebuilding the
  exporter without that feature and re-rendering drops the 34 curl entries. The
  docs extractor refuses that configuration for a reason.
- `Area::Debug` adds a `Debug` category to the builtins master index and area
  list; the audit's `non_registry` and prelude counts must move with it.
- `elephc_dump()`/trace contracts are visible in docs but the names are not
  callable outside a debug build. The page summary says `--debug` explicitly.
- `--strict-php` hides the contracts and skips the prelude; both diagnostics and
  eval visibility derive from the same extension flag.
- The `ValueError` test checks the combined stdout/stderr because an uncaught
  fatal currently prints to stdout in this backend.
