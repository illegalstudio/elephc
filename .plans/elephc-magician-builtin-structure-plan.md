# Plan: elephc-magician Builtin Structure

## Task

- [x] Phase 1: introduce a declarative eval-side registry without changing
  runtime behavior.
- [x] Phase 1: migrate a small pilot set of simple builtins into the new
  per-builtin layout and derive metadata from that registry.
- [x] Phase 1: update parity tests to query the registry instead of searching
  dispatcher string literals for migrated builtins.
- [x] Phase 2: migrate already implemented magician builtins area by area while
  keeping fallback to existing dispatchers until each area is complete.
- [x] Phase 2: remove duplicate manual tables for names, signatures, defaults,
  by-ref parameters, and dispatch in migrated areas.
- [x] Phase 2: keep ordinary files below 500 LoC, leaving exceptions only for
  cohesive single-scope helpers documented in their module preambles.
- [x] Phase 3: split remaining large builtin files (`symbols.rs`,
  `filesystem/streams.rs`, `class_metadata/oop_introspection.rs`,
  `registry/callable.rs`, `arrays/core.rs`) into builtin home files and shared
  helpers.
- [x] Phase 3: replace the giant direct-dispatch match in
  `interpreter/expressions.rs` with smaller registry lookups, preserving special
  paths for language constructs and by-ref/source-sensitive calls.
- [x] Phase 3: update agent/contributor documentation if the workflow for adding
  eval builtins changes.
- [x] Phase 4: convert `array_keys` and `array_values` so each builtin home file
  contains both its `eval_builtin!` declaration and its PHP-visible
  implementation.
- [x] Phase 4: merge the remaining declaration-only builtin home files with
  their PHP-visible direct/by-value implementations, area by area.

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
- the PHP-visible builtin implementation for that entry.

Shared helpers are still allowed for non-PHP-visible common algorithms, but they
must not be a separate per-builtin implementation file with a declaration-only
home file forwarding into it. If a helper exceeds 500 LoC but has one clear
scope, its preamble must explain why keeping it cohesive is better than splitting
it mechanically.

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

Phase 2 completion leaves the procedural date/time alias fallback explicit in
`registry/dispatch/mod.rs`. Eval cannot run the static name-resolver rewrite
before runtime dispatch, so aliases such as date/time procedural names remain an
eval-only runtime bridge rather than duplicated builtin metadata.

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

Phase 3 completion notes:

- `registry/names.rs` and `registry/signature.rs` are now thin registry-derived
  helpers rather than manual tables.
- `interpreter/expressions.rs` no longer contains the giant positional builtin
  dispatch match. Function-like calls live under `interpreter/expressions/calls*`
  and fall through to `eval_declared_builtin_direct_call()` after preserving the
  special source-sensitive and by-reference paths.
- `interpreter/builtins/symbols.rs` is now an orchestration module with focused
  `symbols/` modules for callable probes, function probes, constants,
  class-name lookup, class relations, and language constructs.
- `filesystem/streams.rs`, `class_metadata/oop_introspection.rs`, and
  `arrays/core.rs` have already been split into focused helper modules.
- `filesystem/stream_sockets.rs` was also split during the cleanup because it
  was a multi-builtin stream-socket bucket above the ordinary file-size target.
- `registry/callable.rs`, `registry/callable_validation.rs`,
  `registry/dynamic_mutation.rs`, and `time/aliases.rs` remain above 500 LoC as
  documented single-scope engines in their module preambles.
- No `AGENTS.md` or contributor workflow update was needed for Phase 3 because
  adding eval builtins still follows the existing one-home-file plus area
  `mod.rs` wiring model established by the declarative registry.

## Phase 4: Merge Declarations with Implementations

After Phase 3, several builtins still had a home file that only registered
metadata and then delegated to an implementation helper elsewhere. That is no
longer the target model.

For each migrated builtin:

- keep the `eval_builtin!` declaration in the builtin home file;
- move the PHP-visible direct wrapper into that same file;
- move the PHP-visible evaluated-argument/result wrapper into that same file;
- keep shared helper modules only when they represent a real common algorithm,
  not a one-builtin implementation hidden away from the declaration;
- remove old `declarations/` folders and declaration-only files as each area is
  migrated.

The pilot conversion moved `array_keys` and `array_values` into this stricter
shape and deleted the old shared projection implementation file.

Phase 4 completion notes:

- Declaration-only `eval_builtin!` leaf files have been eliminated. Each
  migrated builtin home file now contains PHP-visible eval functions after its
  registry declaration.
- The remaining shared functions are common algorithms owned by a sibling
  builtin file or shared helper module, not one-builtin implementation files
  hidden away from declaration-only homes.
- Old `declarations/` folders are gone from `crates/elephc-magician`.

## Acceptance Criteria

- Adding an eval builtin requires one home file and at most area `mod.rs` wiring,
  not edits to four separate manual tables.
- A migrated builtin's home file is not declaration-only when
  `elephc-magician` owns that builtin's implementation.
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
