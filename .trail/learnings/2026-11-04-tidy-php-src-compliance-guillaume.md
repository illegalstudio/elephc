---
title: "ext/tidy landed on a managed libtidy 5.8.0 bridge with the full php-src corpus"
author: Guillaume
date: 2026-11-04
branch: feat/php-tidy
touches:
  - crates/elephc-tidy/
  - src/tidy_prelude.rs
  - src/tidy_prelude/
  - src/tidy_prelude/build/
  - src/synthetic_class.rs
  - src/synthetic_class/transcribe.rs
  - src/native_deps/recipes/libtidy.rs
  - crates/elephc-builtin-contract/src/catalog_tidy.rs
  - tests/php_src/tidy/
  - tests/codegen/tidy*
  - scripts/gen_php_src_tidy_cases.py
  - docs/php/tidy.md
---

## Decided
- `tidy` is a crate-backed bridge like `xml`, not a set of registry builtins. The
  entire PHP surface (24 functions, `tidy`/`tidyNode`, `ob_tidyhandler`) is built as
  AST over an `extern "elephc_tidy"` block. The PHP form in
  `src/tidy_prelude/fragments.rs` is `cfg(test)` only: `synthetic_class::transcribe`
  generated `src/tidy_prelude/build/*.rs`, and a node-by-node oracle keeps the two in
  sync. The compiler never tokenizes the tidy prelude.
- libtidy 5.8.0 is built by a reviewed direct-CC recipe (`recipes/libtidy.rs`) from the
  upstream `src/*.c` list. Upstream is CMake-only, but this repository's managed
  toolchain only fingerprints `cc`/`ar`/`ranlib`; compiling the CMake source list by
  hand keeps all five supported targets on the existing toolchain.
- No C shim. `TidyBuffer` is public libtidy ABI and is mirrored in `ffi.rs`; option
  iteration uses libtidy's own iterator. Every return string uses `thread_local!` cells,
  and the handle registry keeps one document reference per live `tidyNode`.
- The `tidy` document handle lives in a private static `spl_object_id()` map, not an
  instance property, so `var_dump()`/reflection on a `tidy` subclass shows only
  `errorBuffer` and `value` (php-src `bug_50558`).
- `tidyNode` properties are readonly and computed eagerly, including the recursive
  `child` array. The public constructor is a guarded shim that throws the php-src
  private-constructor `Error` at runtime; the compiler otherwise rejects the call
  before the test can observe PHP's runtime error.
- `tidy.default_config`, `tidy.clean_output` and `open_basedir` are baked in as
  compile-time prelude constants. The compiler injects tidy for `tidy.clean_output=1`
  even when the source names no tidy symbol (test 026).
- The php-src PHP-8.5.10 `ext/tidy/tests` corpus is committed under
  `tests/php_src/tidy/` and generated into one Rust `#[test]` per fixture.
- Two general compiler gaps surfaced and were fixed rather than papered over:
  private-constructor access is now a runtime `Error` lowered at the call site
  (`ThrowAccessKind::PrivateConstructor`), so the fatal trace has no constructor frame;
  and `$object->staticMethod()` now resolves and lowers through the static-method path
  after evaluating the receiver for side effects.
- Eval support uses 24 one-line Magician `eval_builtin!` homes that forward to the
  host's compiled tidy prelude, plus `EvalArea::Tidy` dispatch, `extension_loaded()`
  and `get_loaded_extensions()` entries. `--with-tidy` is still required when the only
  call is inside opaque eval source.

## Results
- `cargo test --test codegen_tests php_src_tidy`: 49 passed, 1 ignored.
- `parsing_file_too_large.phpt` is the only ignored fixture, for php-src's own
  resource-heavy guard.
- Everything else passes, including `020.phpt`'s Latin-1 output, `035.phpt`'s fatal
  stack trace, and `gh20374`'s NAN/INF warnings.

## Test-only adaptations
- `012.phpt`: dropped the user function's `tidyNode` type hint; elephc deliberately
  does not narrow `mixed` to a class at a call site.
- `020.phpt`: Latin-1 source bytes are written as `\xNN` escapes because Rust source
  cannot carry invalid UTF-8; PHP decodes them to the same bytes.
- The harness restores PHP's display-layer blank line before diagnostics; elephc's
  runtime writes the diagnostic text without it. This mirrors the existing GMP PHPT
  harness.

## Follow-up review fixes (2026-11-05)
- `tidy_repair_string()`/`tidy_repair_file()` now keep repairing after a configuration
  or encoding failure, matching `php_tidy_quick_repair()`'s `RETVAL_FALSE` overwrite.
  A failed configuration load also skips the encoding attempt, matching php-src's
  `else if`.
- Parse paths attempt an empty encoding (`tidySetCharEncoding("")` fails); repair paths
  skip it (`Z_PARAM_STRING`).
- Reparsing a document invalidates old node handles before libtidy releases the tree,
  so stale `tidyNode` wrappers fail closed instead of dereferencing freed memory.
- `tidyNode`'s prelude constructor also throws when entered outside its minting
  wrapper, closing dynamic `new $class` for `tidyNode`; user classes remain tracked by
  `elephc-lia4`.
- Union parameter binding now uses the declared union in EIR, applies PHP's
  int/float/string/bool preference, and has a runtime dispatch for boxed `Mixed`
  places. Strict mode rejects scalar-to-union bindings while keeping exact members and
  int-to-float widening.
- Constructor configuration warnings throw `ErrorException` with E_WARNING/E_NOTICE
  severity, default-config warnings use the caller label, and `tidy.default_config`
  skips php-src's user-config `open_basedir` check.
- The prelude was migrated from parsed PHP to `synthetic_class` AST. The PHP form is
  `cfg(test)` only, the oracle compares it node by node with `build/*.rs`, and the
  generated declarations receive synthetic non-zero spans before checking. That last
  point matters: `array` resolves to raw packed storage at a zero span, which silently
  corrupted `tidy_get_config()`'s associative array until the declarations were stamped.
- `phpversion('tidy')` inside eval now uses the effective extension set, matching
  `extension_loaded()`.
- The iOS tidy compile/link job now builds `elephc-phar` and links the SDK's zlib/bzip2,
  the same stream-layer dependencies the fileinfo link exposed.
- Literal `eval()` source is scanned on identifier boundaries for tidy functions,
  classes, and `ob_tidyhandler`, so the prelude is injected without parsing runtime
  source. The eval output-handler bridge then resolves the host AOT callback and its
  FINAL flush repairs the buffer. Opaque runtime-assembled eval source still needs
  `--with-tidy`, documented in `docs/php/tidy.md`.

## Watch out
- A `--with-tidy` compile still needs `elephc native add libtidy`; there is no system
  `-ltidy` fallback. The CI/release lists now materialize it and fail closed with
  `ELEPHC_TEST_REQUIRE_TIDY_NATIVE=1`.
- `tidy.clean_output`, `tidy.default_config` and `open_basedir` are compile-time only;
  runtime `ini_set()` does not change them. This is documented in `docs/php/tidy.md`.
- The catalog constant table is sorted by exact name; `TIDY_*` sits between
  `SUNFUNCS_RET_TIMESTAMP` and `TRUE`.
- Generated builtin docs now have a dedicated `Tidy` area; rerun the normal
  `update-builtin-docs` sequence after any catalog change.
- Dynamic `new $class` now enforces constructor visibility from the calling lexical
  scope, and constructor-padding thunks resolve the declaring owner so an inherited
  private constructor still runs when the scope allows it. The fixed-class and
  `tidyNode` cases remain handled by `ThrowAccessKind::PrivateConstructor` and the
  prelude minting guard.

## Four-model review pass (2026-11-05)

- Reviewed by GLM-5.3-Flash, MiniMax-M3.1-Flash, Mistral-Large-4, and Kimi (Kimi K3 fallback; OpenRouter K3.1-Pro was unavailable with the configured credentials).
- Fixed release/nightly YAML indentation, the `T:false` weak-union miscompile, nullsafe `$object?->staticMethod()`, EH_THROW constructor severity/class, `tidyNode` class-scope diagnostics, literal callable detection, bare `'tidy'` over-injection, engine-vs-user warning severity, byte-exact PHPT comparison, shared INI boolean parsing, dynamic-new error locations, `tidyNode` attribute filtering, and the stale-node documentation for the pinned PHP 8.5.10 target.
- Added regression coverage for each confirmed defect. Triaged as non-issues for this target: stale-node generation checks (introduced after 8.5.10), eval-shard libtidy skips (the eval shard runner selects only `codegen::eval*`), and the documented `ob_tidyhandler`/`needs_libdl`/constant-catalog adapters.
