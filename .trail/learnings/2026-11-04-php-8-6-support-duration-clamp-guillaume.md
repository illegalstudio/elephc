---
title: "PHP 8.6 support: Duration, clamp mixed, grapheme clusters, readonly defaults"
author: Guillaume
date: 2026-11-04
branch: feat/php-8.6-support
cost: { minutes: 240, tokens: 120000 }
touches:
  - ROADMAP.md
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/lib.rs
  - crates/elephc-builtin-contract/src/object_operators.rs
  - docs/README.md
  - docs/internals/builtins/math/clamp.md
  - docs/php/builtins/math/clamp.md
  - docs/php/builtins/string/grapheme_strrev.md
  - docs/php/compatibility.json
---

Context: adding PHP 8.6 surface to elephc, with php-src PHPT fixtures as the only oracle.

Decided:
- Time\Duration is an AST prelude (src/time_prelude.rs) joined to catalog_classes.rs (route Prelude, module Date, since Php86) with a compare-only ClassOperators contract. The private 3-arg ctor is a documented deviation: `new Time\Duration()` is a compile error in elephc, runtime Error in php-src.
- clamp() now lowers through BuiltinLowering::Eir: int/float/string keep RuntimeFnId::Clamp, and the Mixed result family calls __elephc_clamp() from src/clamp_prelude.rs (php-src's own userland polyfill). The helper copies the selected operand into a local before returning so the callable wrapper can own a Fresh Mixed cell while `===` still sees the same object.
- DateTime/DateTimeImmutable got a compare adapter (timestamp then microsecond) in datetime/interface.rs, wired through DATE_TIME_OPERATORS. Without it, mixed comparisons and clamp() treated every date as equal.
- Readonly property defaults mostly already worked. The real gap was infer_expr_type_syntactic typing `Enum::Case->name` as Int inside a property default; adding the ScopedConstantAccess->name/value arm fixed the whole php-src readonly_with_default fixture.
- Grapheme_Extend needed Arabic/Hebrew/Indic/Thai/Tibetan ranges; the upstream Arabic fixture caught it.

Ruled out:
- Deriving the Duration ISO parser from the RFC prose: timelib differs on `PT0.1S` (parse failure), `P1D` ("may only contain the time (T) aspect"), and the 64-bit overflow messages. The PHPT corpus is the oracle.
- Bundling PHPT files verbatim: the harness shares one global scope, so `$durations` leaked between cases. Wrap each generated case in an IIFE and trim each case's output like run-tests.php.
- Using the generic callable wrapper unchanged for clamp: WrapperBuiltinLoweringContext::emit_user_call was a ConstNull stub. It now emits Op::Call, and the callable-table builder skips clamp when __elephc_clamp is not declared (same pattern as GMP).

Watch out:
- New preludes must be injected in FOUR mirrors, not just pipeline.rs: tests/codegen/support/compiler.rs, tests/error_tests.rs (2 sites), and src/ir_lower/tests/mod.rs. Missing one gives "Unknown type" only under codegen tests.
- src/builtins/parity_tests.rs lists every injected prelude; add new ones there or the structural audit silently skips them.
- Runtime helpers that change by PHP profile are safe because runtime_cache_key includes compile_php_version().version_id().
- php-src's run-tests.php trims both actual and expected output; exact bundle comparison must trim per case.
- Other worktrees run full suites in parallel; codegen binaries hit the 60s ELEPHC_TEST_BINARY_TIMEOUT_SECS under load. Raise it instead of treating timeouts as regressions.
