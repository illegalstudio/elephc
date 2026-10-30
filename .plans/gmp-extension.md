# GMP extension: php-src parity

- [x] **P0. Baseline and conformance harness**
  - [x] Vendor the php-src 8.5.6 `ext/gmp/tests` corpus with provenance and licence notice.
  - [x] Add a PHPT runner that executes `--FILE--` through elephc and compares `--EXPECT--` / `--EXPECTF--`.
  - [x] Record the initial red baseline: every test must fail for a named, tracked reason.
  - [x] Add focused Rust unit tests for the bridge crate before its implementation.
- [x] **P1. Managed native GMP package**
  - [x] Add pinned `gmp` 6.3.0 to the trusted native catalog (source URL, SHA-256, size, archives, headers).
  - [x] Add the autoconf recipe and build it for `macos-aarch64`.
  - [x] Verify `elephc native add gmp` / `native install --locked` and receipt checks.
  - [x] Add the recipe's unit tests and target matrix declarations.
- [x] **P2. `elephc-gmp` bridge crate**
  - [x] Implement the panic-free C ABI over libgmp with a thread-local PHP error channel.
  - [x] Cover arithmetic, division modes, bitwise/shift, roots, number theory, import/export, random state, conversion.
  - [x] Add bridge unit tests pinned to php-src vectors.
  - [x] Wire the staticlib into the bridge table and linker with no system-library fallback.
- [x] **P3. Shared contracts and AOT builtins**
  - [x] Add the 51 procedural contracts plus internal `__elephc_gmp_*` aliases and the `GMP` class contract.
  - [x] Add one generic EIR lowering (boxed operands -> `__elephc_gmp_*` prelude helper), requirements, effects, ownership, and per-family result kinds.
  - [x] Add the `GMP_*` constants and `GMP_VERSION`.
  - [x] Add codegen and error tests (PHPT corpus, bridge tests, dynamic-call coverage) for each builtin family.
- [x] **P4. GMP object semantics**
  - [x] Build the `final class GMP` prelude: constructor, hidden storage, `__debugInfo`, serialization, clone.
  - [x] Implement `(string)`, `(int)`, `(float)`, `(bool)` and `_IS_NUMBER` casts.
  - [x] Implement `+ - * / % ** | & ^ << >> ~`, unary `-`/`+`, comparisons, `++`/`--`, compound assignment and concatenation.
  - [x] Implement PHP 8.5 operand validation: weak/strict float rules, null/array/object/resource diagnostics, division/modulo errors, shift/exponent errors.
  - [x] Integrate GMP comparisons with `sort()`, `min()`, `max()`, `array_sum()` and mixed numeric contexts.
  - [x] Implement dynamic properties and both serialization wire formats (`O:` and legacy `C:`).
- [ ] **P5. Eval and Magician** — explicitly pending: the 51 contracts are in `EVAL_IMPLEMENTATION_PENDING`, tracked as its own Beads feature
  - [ ] Add `eval_builtin!` bindings for all 51 functions.
  - [ ] Add class creation, casts, operators, serialization and constants to the interpreter.
  - [ ] Add AOT/eval parity tests for the whole surface.
- [ ] **P6. CLI, CI, docs and release gates**
  - [x] Add `--with-gmp`, auto-detection and the managed-package link requirement.
  - [x] Materialise and cache GMP in CI for every target that runs GMP tests (codegen shards + bridge tests).
  - [x] Write `docs/php/gmp.md`, update README, compatibility counts and the example.
  - [x] Run the builtin docs generation/audit sequence and commit the outputs (generated; commit pending with the delivery).
  - [x] Final focused verification: 97/97 PHPT corpus cases green on macos-aarch64 (2 documented exclusions), bridge/unit tests green, docs audits green.

## Source of truth

- Reference engine: the php-src 8.5.6 checkout already present in the workspace
  (`scratchpad/php-8.5.6/php-src-php-8.5.6/ext/gmp`), run against PHP 8.5.10
  when a live cross-check is needed.
- Corpus: all 99 `ext/gmp/tests/*.phpt` files, including the 8.5 cryptography,
  overloading, constructor, `final` and serialization additions. The older 8.3
  tests are not sufficient: 8.5 changed float operators, bool casts and added
  the `GMP` constructor.
- The PHPT files, not this plan, decide expected output. Any deliberate
  divergence is recorded in the runner's exclusion table with a reason and a
  follow-up issue, never by loosening an assertion silently.

## Architecture decision

Use the real `libgmp` through the existing managed-native + bridge machinery,
not a Rust bigint reimplementation.

- php-src itself is a thin adapter over libgmp; matching its observable
  behaviour exactly (division modes, infinite two's-complement bitwise
  operations, `mpz_get_si` truncation, `gmp_export` word layouts, the MT-based
  `gmp_random_seed` stream, probable-prime categories) is only tractable by
  calling the same library.
- The 8.5 cryptography tests perform thousands of 512/8192-bit modular
  exponentiations. A pure-Rust bigint would change both the results' edge cases
  and the CI budget.
- This follows the `curl` and `libxml2` precedents: a pinned archive, a
  curated recipe, a reviewed archive order, no system fallback, and a
  fail-closed `elephc native add gmp` recovery.
- GMP is LGPL-3.0-or-later. The recipe builds a static archive, so the bridge
  stays consistent with the rest of the native catalog; downstream distribution
  obligations are documented in `docs/compiling/native-dependencies.md`.

## How the pieces fit

```
PHP source
  -> GMP prelude class + procedural builtins (contract / checker / EIR)
  -> __elephc_gmp_* internal string ABI over RuntimeFnId
  -> elephc-gmp staticlib (C ABI, panic containment, PHP error channel)
  -> managed libgmp.a
```

The prelude class stores the value as a decimal string so the compiler's
existing object model, clone, dynamic-property and property-hash machinery
apply unchanged. `__debugInfo()` projects only `num`, as php-src does, and
`__serialize()` / a legacy `unserialize()` hook reproduce both wire formats.

Operator dispatch reuses `ClassOperators`, extended with the bitwise, shift,
unary and cast adapters GMP needs. Every adapter is a static method taking the
left then right operand, preserving PHP evaluation order. Unsupported operand
types reach the adapter at run time so the exact php-src `TypeError` /
`ValueError` messages and weak/strict coercion rules stay in one place.

## Test strategy

1. **Red first.** Add the vendor corpus and runner before the bridge exists.
   Each red test is assigned to a phase in the tracking issue.
2. **Bridge vectors.** Every bridge entry point has a Rust test whose expected
   value was produced by PHP 8.5.10 + libgmp 6.3.0.
3. **PHPT slices.** The runner supports `--FILE--`, `--EXPECT--`,
   `--EXPECTF--`, `--SKIPIF--` and `--INI--`; SKIPIF is interpreted only for
   platform facts (64-bit, memory) so the suite stays deterministic.
4. **Object handles.** `object(GMP)#N` numbers are normalised only when the
   php-src file itself uses `%d`; exact handles are attempted first and any
   normalisation is documented. Semantic output (properties, values, types)
   is never normalised.
5. **AOT/eval parity.** Every procedural family is exercised in both backends
   through the existing bundled-eval pattern.
6. **Target matrix.** Host PHPT execution proves semantics; CI compile/link
   checks cover `linux-x86_64`, `linux-aarch64`, `macos-aarch64`,
   `ios-arm64` and `ios-sim-arm64`.

## Notes and constraints

- `GMP` is final and has a real constructor in 8.5
  (`new GMP(int|string $num = 0, int $base = 0)`).
- GMP has no declared properties: `num` is debug-info only, and dynamic
  properties are allowed but deprecated.
- Casts: string = decimal, int = `mpz_get_si` (wrapping), float =
  `mpz_get_d`, bool = sign != 0, `_IS_NUMBER` = int when it fits else float.
- Operators in coercive mode accept int/string/GMP and truncate floats (with
  `Deprecated: Implicit conversion from float ...` when fractional); strict
  types reject floats. Null/arrays/resources/foreign objects throw, except
  that comparison against null uses ordinary object-versus-null semantics.
- `gmp_random_seed()` must keep libgmp's process-global generator state; the
  bridge owns the state, not the generated program.
- `gmp_setbit()` / `gmp_clrbit()` mutate the receiver object; these are the
  only two by-reference builtins in the extension.
- `array_sum([$gmp, 1])` depends on the `_IS_NUMBER` cast and is a named 8.5
  regression test (`gh16890`).
- `gmp_export()` overflow and `gmp_random_bits(PHP_INT_MAX)` are named 8.5
  regressions (`gh16411`, `gh16501`).
