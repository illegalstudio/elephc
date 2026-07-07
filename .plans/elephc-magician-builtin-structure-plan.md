# Plan: elephc-magician Builtin Structure

## Task

- [x] Phase 1: introduce a declarative eval-side registry without changing
  runtime behavior.
- [x] Phase 1: migrate a small pilot set of simple builtins into the new
  per-builtin layout and derive metadata from that registry.
- [x] Phase 1: update parity tests to query the registry instead of searching
  dispatcher string literals for migrated builtins.
- [ ] Phase 2: migrate already implemented magician builtins area by area while
  keeping fallback to existing dispatchers until each area is complete.
- [ ] Phase 2: remove duplicate manual tables for names, signatures, defaults,
  by-ref parameters, and dispatch in migrated areas.
- [ ] Phase 2: keep ordinary files below 500 LoC, leaving exceptions only for
  cohesive single-scope helpers documented in their module preambles.
- [ ] Phase 3: split remaining large builtin files (`symbols.rs`,
  `filesystem/streams.rs`, `class_metadata/oop_introspection.rs`,
  `registry/callable.rs`, `arrays/core.rs`) into builtin home files and shared
  helpers.
- [ ] Phase 3: replace the giant direct-dispatch match in
  `interpreter/expressions.rs` with smaller registry lookups, preserving special
  paths for language constructs and by-ref/source-sensitive calls.
- [ ] Phase 3: update agent/contributor documentation if the workflow for adding
  eval builtins changes.

## Goal

`elephc-magician` should move closer to the builtin model used by `elephc`: one
home file per builtin, metadata next to the implementation, and dispatch derived
from one source of truth. The crate remains a separate eval bridge; it should not
depend directly on the main `elephc` crate only to share compiler-internal types.

The goal is not to copy `src/builtins/` mechanically. It is to replicate the
useful properties:

- builtin declarations live next to the code they implement;
- names, parameters, defaults, by-ref flags, variadics, and dispatch are derived
  from one source;
- files stay small and cohesive;
- exceptions are justified only for single-scope engines/helpers.

## Current State

`elephc` uses `src/builtins/<area>/<name>.rs` with the `builtin!` macro. That
file drives catalog lookup, signatures, type checking, lowering, and generated
documentation.

`elephc-magician` currently has several manual sources:

- `crates/elephc-magician/src/interpreter/builtins/registry/names.rs` for the
  PHP-visible builtin list;
- `crates/elephc-magician/src/interpreter/builtins/registry/signature.rs` for
  signature shape, defaults, and by-ref metadata;
- `crates/elephc-magician/src/interpreter/expressions.rs` for direct dispatch of
  positional builtin calls;
- `crates/elephc-magician/src/interpreter/builtins/registry/dispatch/*.rs` for
  dynamic/by-value dispatch;
- family files under `interpreter/builtins/` for the actual implementations.

This duplication makes adding a builtin expensive and makes parity tests depend
on string literals inside dispatchers.

## Target Layout

Suggested layout:

```text
crates/elephc-magician/src/interpreter/builtins/
  macros.rs
  spec.rs
  registry/
    mod.rs
    binding.rs
    callable.rs
    dynamic_mutation.rs
  array/
    count.rs
    array_map.rs
    array_reduce.rs
    helpers.rs
  string/
    strlen.rs
    strrev.rs
    substr.rs
    helpers.rs
  types/
    boolval.rs
    intval.rs
    is_array.rs
  filesystem/
    fopen.rs
    fread.rs
    stream_helpers.rs
```

Each builtin home file should contain:

- a Rustdoc module preamble;
- the `eval_builtin!` declaration;
- a direct-call wrapper over `EvalExpr` arguments, when needed;
- a dynamic/by-value wrapper over `RuntimeCellHandle`, when needed;
- a mutating/by-ref wrapper, if the builtin can write into caller storage;
- delegation to shared helpers when multiple builtins use the same algorithm.

Shared helpers must not become generic buckets. If a helper exceeds 500 LoC but
has one clear scope, its preamble must explain why keeping it cohesive is better
than splitting it mechanically.

## Phase 1: Declarative Registry Without a Big Bang

Introduce eval-side infrastructure alongside the existing code.

Components:

- `spec.rs` with `EvalBuiltinSpec`, `EvalArea`, parameters, defaults, by-ref
  flags, variadics, and dispatch hooks;
- `macros.rs` with `eval_builtin!`, modeled on the main `builtin!` macro but
  using magician-specific types;
- `registry/mod.rs` with case-insensitive lookup, ordered name iteration, and
  conversion into `builtin_metadata`;
- compatibility with old dispatchers: an unmigrated builtin continues through
  the existing match path.

Suggested pilot builtins:

- `strlen`;
- `count`;
- `boolval`;
- `abs`;
- `strrev`.

They cover strings, arrays/countable objects, casts, math, and string runtime
helpers, while staying small enough for a readable first PR.

Minimum checks:

- `cargo test -p elephc-magician <pilot_builtin_filter>`;
- `cargo test --test builtin_parity_tests`;
- `git diff --check`.

## Phase 2: Area-by-Area Migration

Migrate one area at a time without changing behavior.

Suggested order:

1. `types` and scalar casts/predicates;
2. simple math/formatting builtins;
3. stateless string builtins;
4. non-mutating array builtins;
5. JSON, regex, and time;
6. filesystem/stream builtins;
7. symbols/reflection/class metadata;
8. by-ref/mutating and callable special cases.

For each area:

- create home files for each builtin;
- move metadata into `eval_builtin!`;
- derive names/signatures/defaults/by-ref data from the registry;
- convert the area's dynamic dispatch to registry lookup;
- reduce direct dispatch to registry lookup or area-scoped dispatch;
- update parity tests so they do not depend on `include_str!` over legacy files;
- keep the area's focused tests green.

During Phase 2, a hybrid system is acceptable: the new registry for migrated
areas, manual dispatchers for areas not yet migrated. Duplicating metadata for a
migrated builtin is not acceptable.

## Phase 3: Large-File Cleanup and Legacy Path Removal

Once most builtins use the registry:

- remove or reduce `registry/names.rs`;
- reduce `registry/signature.rs` to common helpers or remove it;
- split `registry/dispatch/*.rs` files that have become long matches only;
- split `interpreter/expressions.rs`, leaving only:
  - language constructs (`eval`, `isset`, `unset`, `empty`);
  - source-sensitive or by-ref calls that cannot pre-evaluate every argument;
  - ordered fallback into registry direct-call dispatch.

Files that need explicit treatment:

- `interpreter/builtins/symbols.rs`;
- `interpreter/builtins/filesystem/streams.rs`;
- `interpreter/builtins/class_metadata/oop_introspection.rs`;
- `interpreter/builtins/registry/callable.rs`;
- `interpreter/builtins/arrays/core.rs`.

For each one, decide whether:

- it is a multi-builtin bucket that should be split into home files;
- it is a single-scope helper that should stay cohesive with an explicit
  preamble;
- it mixes responsibilities and should be split before builtin migration.

## Acceptance Criteria

- Adding an eval builtin requires one home file and at most area `mod.rs` wiring,
  not edits to four separate manual tables.
- `elephc_magician::builtin_metadata` is derived from the registry and stays
  aligned with parity tests.
- Parity tests compare the static registry and eval registry, not string
  literals scattered through dispatchers.
- Ordinary files stay below 500 LoC; single-scope exceptions are justified in
  module preambles.
- Work can land area by area, and every PR leaves the relevant focused tests
  green.

## Risk Notes

- By-ref builtins and dynamic callables preserve evaluation order and writable
  targets. Migrate them after simple by-value builtins.
- `elephc-magician` is an ABI-facing staticlib; do not expose unstable Rust
  types across the C boundary.
- Avoid a direct dependency on `elephc` for sharing `BuiltinSpec` unless there is
  an explicit future decision to extract a common crate.
- Do not perform a mechanical migration without tests. The main risks are
  breaking named arguments, spread arguments, defaults, and PHP-compatible
  warnings.
