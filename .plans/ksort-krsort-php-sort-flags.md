# PHP sort flags for `ksort()` and `krsort()`

> **Implemented.** This plan is kept as the design record; the sections below describe the
> state BEFORE the change. Three things landed differently, and each is documented where it
> lives:
>
> - The constants went into the shared catalog (`crates/elephc-builtin-contract/src/
>   catalog_constants.rs`), not `src/types/array_constants.rs`, which no longer exists.
> - `$flags` renders as `int $flags = 0` in the generated signature rather than
>   `SORT_REGULAR`: a symbolic default cannot reach an eval registry binding, and both key
>   sorts have one.
> - `SORT_LOCALE_STRING` clips each operand at the first NUL and compares bytes instead of
>   calling libc `strcoll`. That IS `strcoll` in the C locale, which is the only locale a
>   program without a PHP-visible `setlocale()` can be in, and calling libc would mean two
>   heap allocations per comparison to manufacture the NUL-terminated operands it wants.

## Context

Elephc currently exposes unary key-sort signatures:

```php
ksort(array &$array): bool
krsort(array &$array): bool
```

Both AOT runtime helpers always use PHP's default `SORT_REGULAR` comparison. PHP 8.4 exposes an
optional integer flag on both functions:

```php
ksort(array &$array, int $flags = SORT_REGULAR): true
krsort(array &$array, int $flags = SORT_REGULAR): true
```

PR #695 makes the existing default-mode implementation ownership-safe and gives both directions
the same nested `array<mixed>[int]` checker, COW, write-back, and runtime-TypeError behavior. This
plan adds the missing comparison modes without reopening those ownership paths.

## Goal

- Accept the optional positional or named `$flags` argument in direct AOT calls, direct `eval()`
  calls, first-class/static callable metadata, and runtime-selected builtin calls.
- Implement PHP-compatible key comparison for `SORT_REGULAR`, `SORT_NUMERIC`, `SORT_STRING`,
  `SORT_LOCALE_STRING`, `SORT_NATURAL`, and the supported `SORT_FLAG_CASE` combinations.
- Preserve source-order evaluation, by-reference mutation, COW separation, packed-to-hash
  promotion for descending key order, and the nested Mixed-cell write-back introduced in PR #695.
- Support `macos-aarch64`, `linux-aarch64`, and `linux-x86_64` in the same change.
- Document the exact locale boundary and prove AOT/eval behavior against PHP-generated fixtures.

## Non-goals

- Adding `setlocale()` or a general locale subsystem. `SORT_LOCALE_STRING` observes the process
  locale; without a PHP-visible locale mutator, Elephc's reachable behavior is the C locale.
- Adding `$flags` to `sort()`, `rsort()`, `asort()`, or `arsort()` in this change. The comparison
  machinery should be reusable by that follow-up, but this issue is scoped to key sorting.
- Changing PHP's key normalization rules or supporting array keys other than normalized `int` and
  `string` keys.
- Replacing the stable insertion-order relinking algorithm or changing key/value ownership.

## PHP contract to pin

Add these values to the existing `ARRAY_INT_CONSTANTS` source of truth:

| Constant | Value |
|---|---:|
| `SORT_REGULAR` | 0 |
| `SORT_NUMERIC` | 1 |
| `SORT_STRING` | 2 |
| `SORT_LOCALE_STRING` | 5 |
| `SORT_NATURAL` | 6 |
| `SORT_FLAG_CASE` | 8 |

Do not assume unknown bits or unusual combinations raise `ValueError`. PHP 8.4 accepts values such
as `SORT_NUMERIC | SORT_FLAG_CASE` and `999`; build golden fixtures for runtime-computed modes and
copy PHP's effective mode selection and ignored-bit behavior.

The fixture matrix must distinguish at least:

- integer keys `2` and `10` under numeric, string, and natural comparison;
- numeric-string-looking keys that remain strings, including leading `+`, leading zeroes,
  whitespace, decimals, exponents, overflow-sized integers, and embedded NUL bytes;
- ASCII case pairs and digit runs under `SORT_STRING | SORT_FLAG_CASE` and
  `SORT_NATURAL | SORT_FLAG_CASE`;
- mixed integer/string keys under every mode;
- stable ordering when two keys compare equal;
- empty and single-entry hashes;
- omitted, positional, named, constant, folded, and runtime-computed `$flags`.

## Design

### 1. Constants and builtin signatures

- Extend `src/types/array_constants.rs`; checker registration, name resolution, and codegen
  materialization already consume `ARRAY_INT_CONSTANTS`.
- Change both `builtin!` declarations to
  `params: [ref array: Mixed, flags: Int = DefaultSpec::Int(0)]`.
- Change both `eval_builtin!` declarations to add
  `flags = EvalBuiltinDefaultValue::Int(0)`.
- Verify direct and first-class callable signatures, named argument binding, arity diagnostics, and
  generated registry parity.

### 2. Backend-neutral flag mode

Introduce one typed key-comparison mode shared by AOT lowering and its tests, rather than sending
PHP constant names into codegen. It should retain the raw integer when PHP's ignored-bit behavior
matters and resolve it to one of:

- regular;
- numeric;
- binary string;
- locale string;
- natural string;
- case-insensitive string;
- case-insensitive natural string.

Keep Magician's Rust implementation independent, but drive both sides from the same PHP-golden
fixture inventory so the two backends cannot silently diverge.

### 3. Argument and ref-place lowering

The current reverse-key argument path promotes a packed local only when exactly one argument is
present. Extend it to preserve the optional flag operand while still promoting argument zero.

The nested ref-place helpers currently identify a single receiver argument. Refactor them to bind
`$array` and `$flags` through the shared call-argument plan so all of these retain identical source
evaluation order and write-back behavior:

```php
krsort($grid[index_expr()], flags_expr());
krsort(flags: flags_expr(), array: $grid[index_expr()]);
ksort(array: $grid[index_expr()], flags: flags_expr());
```

The array place and flag expression must each be evaluated once. COW/promotion failure must not
drop an already-observable side effect, and nested scalar/missing cells must keep the correct
`ksort()` or `krsort()` TypeError.

### 4. AOT runtime ABI and comparator dispatch

- Allow one or two operands in `lower_array_key_sort`; materialize `SORT_REGULAR` when omitted.
- Extend the `__rt_hash_ksort` / `__rt_hash_krsort` ABI with a flag word and preserve it across the
  shared insertion-sort loop on both target architectures.
- Keep `__rt_hash_sort_links` responsible only for stable relinking. Select a typed key comparator
  before or inside the comparison step without moving buckets or changing refcounts.
- Retain `__rt_key_compare_regular` unchanged as the default comparator.
- Add focused helpers for numeric, binary/case-folded string, natural/case-folded natural, and
  locale comparison. Reuse numeric-string parsing and integer formatting helpers where their
  contracts match; do not round integer keys through `f64` when exact integer ordering is required.
- For `SORT_LOCALE_STRING`, call the target libc collation primitive under the current process
  locale and document that Elephc has no PHP-visible `setlocale()` yet. Keep length/NUL behavior
  aligned with php-src fixtures.
- A packed `ksort()` remains an ascending-key no-op but still evaluates and binds `$flags`.
  A non-empty packed `krsort()` still promotes to an integer-keyed hash before comparison.

### 5. Magician parity

- Extend direct and dynamic mutating-call binders to accept the optional flag while keeping the
  first parameter by reference.
- Pass the resolved mode into `eval_array_key_sort_entries` and construct mode-specific keys.
- Reuse the existing Rust natural-order comparison only after its behavior matches the shared PHP
  fixture inventory; extend it for integer-key stringification, case folding, locale behavior, and
  mixed key domains as required.
- Cover direct calls, `call_user_func()`, `call_user_func_array()`, first-class callables, named
  arguments, and runtime-selected flags.

### 6. Tests and documentation

Add focused coverage for:

- AOT key-sort outputs and stable equal comparisons in `tests/codegen/arrays/`;
- nested local/property/element mutation, packed promotion, COW aliases, named arguments, and
  scalar/missing TypeErrors;
- wrong flag types and one-to-two-argument arity diagnostics in `tests/error_tests/`;
- Magician direct and dynamic/callable paths;
- constant exposure and case-sensitive constant names;
- all three supported targets through focused local tests where useful and the complete CI matrix.

Update the existing associative-array example to demonstrate at least numeric versus string key
ordering. Regenerate and audit builtin docs with the `update-builtin-docs` workflow. User docs must
state the C-locale boundary of `SORT_LOCALE_STRING` rather than claiming mutable-locale parity.

## Suggested implementation sequence

1. Add PHP-golden fixtures and constant/signature tests that fail on the current unary contract.
2. Add constants and optional parameters across AOT and Magician registries.
3. Make argument/ref-place lowering preserve the flag without changing ownership behavior.
4. Implement AOT comparator dispatch and all supported modes on both architectures.
5. Implement Magician modes against the same fixture inventory.
6. Run focused ownership/GC, callable, eval, and target tests; regenerate docs and update the
   example/changelog.

## Acceptance criteria

- Every documented flag and observed PHP combination produces fixture-identical key iteration
  order for both `ksort()` and `krsort()` in AOT and `eval()`.
- Omitted flags remain byte-for-behavior compatible with the current `SORT_REGULAR` implementation.
- The optional flag does not regress nested Mixed lvalues, COW aliases, packed `krsort()` promotion,
  named-argument evaluation order, or builtin-specific TypeErrors.
- Generated builtin signatures show `int $flags = SORT_REGULAR` semantically (the registry may
  render the literal default as `0` if symbolic defaults are not yet supported).
- Focused local validation is green and CI passes on macOS ARM64, Linux ARM64, and Linux x86_64.
