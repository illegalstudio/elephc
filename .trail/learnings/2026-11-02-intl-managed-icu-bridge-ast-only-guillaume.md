---
title: "Intl: managed ICU bridge, AST-only declarations, and php-src PHPT migration"
author: Guillaume
date: 2026-11-02
branch: perf/runtime-cache-content-and-build-script
cost: { minutes: 180, tokens: 90000 }
touches:
  - src/intl/mod.rs
  - src/intl/functions.rs
  - src/intl/class_constants.rs
  - crates/elephc-intl/src/lib.rs
  - src/native_deps/recipes/elephc_icu_intl_shim.cpp
  - crates/elephc-builtin-contract/src/catalog_intl.rs
  - tests/codegen/intl_php_src.rs
  - tests/codegen/intl_php_src_cases.rs
  - scripts/gen_php_src_intl_cases.py
  - tests/php_src/intl/phpt_status.json
---

## Decided
- Intl is implemented as a managed ICU 78.3 native package (`icu`) plus a Rust `staticlib`
  (`elephc_intl`) forwarding a fixed C ABI to `libelephc_icu_intl_shim.a`. The shim is C++,
  owns ICU objects in a mutex-protected handle table, and uses ICU's C++ API. `elephc native
  install --target …` materializes it; ordinary programs link it only when an Intl symbol is
  referenced.
- No PHP source is parsed for the Intl surface. `src/intl/mod.rs` builds every declaration with
  `crate::synthetic_class` (`function`, `class`, `method`, extern nodes). Scalar-only procedural
  functions are registry builtins; object-bearing/by-reference functions and all classes are
  synthetic `FunctionDecl`/`ClassDecl` nodes. The only PHP text near Intl is the committed
  php-src PHPT corpus, parsed by the test generator, never by the compiler or test runtime.
- The registry builtin lowering uses one generic EIR hook: build an indexed `Mixed` array of
  normalized operands, call a typed `__elephc_intl_invoke_*` helper, and return its result.
  Locale builtins use `__elephc_intl_invoke_locale` so a runtime NUL guard can throw PHP's
  `ValueError`. The helper functions are rooted by `crate::intl::root_function_names()` because
  the AST reachability scan cannot see EIR-only call edges.
- The php-src migration is `scripts/gen_php_src_intl_cases.py` → inline Rust tests in
  `tests/codegen/intl_php_src_cases.rs`; `tests/php_src/intl/` is provenance plus
  `phpt_status.json` (status/reason from the reference PHP 8.5.10 + ICU 78.3 run). The test
  process never reads a `.phpt`.

## Tried and ruled out
- A PHP-source prelude: rejected by the user's AST-only constraint, and the repo's
  `synthetic_class` docs show checker-injected `FlattenedClass` bodies never reach lowering.
- One generic string RPC for every method: still used at the C ABI, but constructing class
  methods through typed AST builders is simpler than encoding every PHP call shape in a
  prelude.
- Using system ICU: fail-closed policy; the managed `icu` package is required. The system ICU
  installation is only used as a local development oracle.
- Typing every `ut_common.inc` wrapper as `mixed`: elephc's checker rejects `mixed` where an
  object/string/array parameter is expected. Concrete types or a future runtime weak-mode check
  are required.

## Watch out
- Changing `elephc_icu_intl_shim.cpp` does not change the native artifact key. Bump
  `ICU_VERSIONS.recipe_revision` (catalog + `recipe.rs` + `icu.rs` doc) and run
  `elephc native install` before re-testing; otherwise the old shim archive is reused.
- The test harness's `tests/codegen/support/compiler.rs` is a second pipeline. Intl must be
  injected there too (before the pack prelude), and the forced group + root names must be added.
- `IntlIterator` method bodies and any generated closure need concrete types; `usort`/`uasort`
  reject `Mixed` callbacks. Build a `[]` array, copy elements in a `foreach`, and sort that.
- Generated `IntlParam`/`IntlFunction` fields are data for the prelude builder; unused fields
  carry `#[allow(dead_code)]`.
- Current ledger after this session: 45 pass / 389 fail / 48 ignored of 482 PHPTs. The failing
  clusters are tracked by `elephc-dci.7.1`–`elephc-dci.7.6`. Locale has 19 passing, dateformat
  3, and the rest are still open.
- Synthetic helpers that return `mixed` MUST declare `.returns(t_mixed())`. Without an explicit
  return type the checker infers the first branch (often int) and then coerces later array
  values to int while building result hashes. This silently turned `getAllVariants()` strings
  into zeros until `__elephc_intl_decode_value`, `__elephc_intl_wrap`, `__elephc_intl_invoke*`,
  and `__elephc_intl_handle_arg` all declared `mixed` returns.

## Second pass (Calendar/TimeZone/DateFormatter)

- THE BRIDGE MUST CALL ICU ONCE. The original `elephc_intl_call` did a null-buffer sizing
  call and then the real call; every mutator (`add`, `roll`, `set`, `setTimeZone`,
  `locale_set_default`, `collator_create`) ran twice. `add(DAY_OF_MONTH, 1)` advanced two
  days. `crates/elephc-intl/src/lib.rs` now allocates one 1 MiB buffer and calls once; an
  oversized result fails closed instead of replaying a mutation.
- A `(array)` cast of a boxed `mixed` keeps the boxed storage shape; reading it as a PHP
  array yields header words. `__elephc_intl_invoke_array` rebuilds the array element by
  element (`foreach` + `$result[$k] = $v`).
- A nullable scalar packed into the bridge's `mixed` argument array loses its tag and is
  encoded as `s` (empty string). Split nullable calls into explicit branches (`clear(null)`
  vs `clear($field)`), or coalesce to a concrete int/float before packing.
- Registry builtins whose name starts with `intlcal_`, `intltz_`, `datefmt_`, `numfmt_`,
  `msgfmt_`, `collator_`, `resourcebundle_`, `transliterator_`, or `idn_` need the bridge
  family of the component, not the PHP prefix (`intlcal_*` -> `intlcal`, `intltz_*` ->
  `timezone`, ...). Procedural instance calls share the object body: the shim strips the
  prefix and dispatches the same method as the OO surface.
- `IntlCalendar` milliseconds: `getTime()`/`setTime()`/`getNow()` take and return **ms**;
  `IntlDateFormatter::format()` takes **seconds** and `parse()` returns **seconds**.
- `IntlGregorianCalendar::createFromDate()/createFromDateTime()` use the default timezone,
  not UTC.
- `IntlCalendar::set()` with 2 args is field/value (the stub names are `$year`/`$month`,
  which is what the ValueError text uses); 3 args is date; 4 is an ArgumentCountError;
  5/6 are date-time. `setDateTime(..., 5 args)` must leave SECOND unset.
- `IntlTimeZone` print_r/var_dump needs a **foldable** `__debugInfo()` projection reading
  four real properties (`valid`, `id`, `rawOffset`, `currentOffset`); a computed body is
  ignored for internal/synthetic classes.
- ICU error messages record "U_ZERO_ERROR" on success. Argument-validation failures must
  store the plain message (no `: U_...` suffix), because PHP's ValueError has no ICU code.
- Measured ledgers after this pass (each suite uses the latest sources; the native artifact
  may lag by one revision because installs are manual):
  - calendar: 49/66 (17 failures: 14 ValueError/Edge parity + PST/CET/WEST DateTime gaps)
  - timezone: 7 passed / 38 failed / 5 ignored (50)
  - dateformat: 3 passed / 25 failed / 11 ignored (39)
  - earlier full-suite baseline before this pass: 45 passed / 389 failed / 48 ignored (482)
  - clean `origin/main` rebase (PR #2221): 121 passed / 313 failed / 48 ignored (482)
- Biggest remaining blockers found in the TimeZone/DateFormatter clusters:
  - `checker` rejects reading a variable that only exists because it was passed by reference
    (`$tz->getOffset($ts, true, $raw, $dst)` / `parse($s, $offset)`). PHP creates it.
    `call_validation.rs`, `resolution/call.rs`, `resolution/resolved.rs` now avoid inferring
    the undefined local, but the output is still not published into the caller env.
  - ext/date's free-form parser does not resolve abbreviations (`PST`, `CET`, `WEST`), so
    the DateTime-based Intl fixtures fail before reaching Intl code.
  - `IntlTimeZone`/`IntlCalendar` loose `==` compares declared properties including the
    hidden handle, so equal ICU objects compare unequal.
- Rebasing the Intl change onto current `main` (PR #2221) required merging with the
  concurrent GMP/ctype/fileinfo work: keep both catalog entries, both recipe imports/arms,
  and both reachability loops. The Intl helpers must not depend on `pack_prelude` (not in
  `main`): encode the 32-bit length with `chr()` instead of `pack('N', ...)`.
- The committed `intl_php_src_cases.rs` uses escaped Rust string literals, not raw strings:
  raw literals put fixture trailing whitespace at Rust line ends and `git diff --check`
  rejects them. `tests/php_src/intl/**` is marked `-text -whitespace` because it is
  verbatim upstream data.
- PR #2221 is a draft. Current deltas vs. the older branch base: current `main` does not
  prepend a blank line before `Deprecated:` (four deprecation-pattern fixtures) and lacks
  the `LC_ALL` constants pulled in by the unrelated `locale_prelude` (one fixture).
