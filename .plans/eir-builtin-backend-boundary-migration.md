# Plan: Backend-Neutral Builtin to EIR Boundary

## Checklist

- [x] Keep a reproducible per-builtin inventory covering the registry, signatures,
  validation, checker and EIR result types, effects, ownership/aliasing, lowering,
  runtime helpers, bridge requirements, callable/eval support, targets, and tests.
- [x] Introduce one registry-owned semantic model for validation, result typing,
  effects, ownership/aliasing, requirements, callable policy, and lowering strategy.
- [x] Introduce stable typed runtime function identifiers and central runtime
  descriptors with logical ABI, effects, ownership, requirements, and target support.
- [x] Add a backend-neutral builtin lowering context that can emit EIR primitives,
  control flow, and typed runtime calls without importing codegen concepts.
- [x] Migrate scalar predicates and conversions.
- [x] Migrate math builtins.
- [x] Migrate string builtins.
- [x] Migrate array builtins.
- [x] Migrate callbacks and sorting builtins.
- [x] Migrate object, class, and reflection builtins.
- [x] Migrate date and time builtins.
- [x] Migrate JSON, serialization, regex, and hash builtins.
- [x] Migrate filesystem, stream, and I/O builtins.
- [x] Migrate process, system, and environment builtins.
- [x] Migrate bridge-backed builtins.
- [x] Migrate pointer, buffer, and elephc extension builtins.
- [x] Migrate internal and prelude builtins.
- [x] Unify direct, first-class, dynamic string, callable-array,
  `call_user_func*`, and eval/Magician callable paths around the same semantics.
- [x] Remove `BuiltinSpec` assembly hooks and every `src/builtins` dependency on
  `src/codegen`.
- [x] Remove registry-backed `Op::BuiltinCall`, the separate EIR return-type chain,
  legacy signature/checker fallback, duplicate effects and requirements matching,
  and separate callable assembly wrappers.
- [x] Add structural invariants plus registry, EIR, optimizer, ownership, callable,
  runtime/linking, and supported-target regression coverage.
- [x] Update contributor and internal architecture documentation, regenerate builtin
  documentation, and add the user-facing changelog entry.
- [x] Run all requested build, test, docs, assembly-comment, EIR, target, and diff
  gates; audit every objective requirement against current evidence.
- [x] Package the completed, green migration as thematic local commits without
  rebasing, amending published history, pushing, or opening a pull request.

## Baseline Inventory

The read-only baseline inventory was taken on 2026-07-20 before the first source
change. The worktree was clean and `feat/eir-migration` was exactly aligned with
`origin/main` at `4e428e6c2` (`0` commits ahead, `0` behind).

The canonical `gen_builtins --include-internal` export contained:

- 462 AOT registry-backed builtins;
- 13 AOT compiler-resident non-registry constructs/aliases;
- 4 eval-only builtins;
- 419 AOT builtins supported through the eval/Magician registry or date alias path;
- 43 AOT builtins without eval support;
- 25 extension builtins;
- 22 internal builtins;
- 28 builtins with at least one by-reference parameter;
- 22 variadic builtins.

All productive registry declarations still carried an assembly `lower` hook.
`src/builtins/**` imported `FunctionContext`, `Instruction`, `CodegenIrError`, or
`crate::codegen` thousands of times. Direct calls still emitted `Op::BuiltinCall`,
`call_return_type` and its helper chain recalculated EIR result types, optimizer
purity lived in a separate per-name list, runtime requirements were partly recorded
through checker hooks, and dynamic callable assembly remained separately generated.

The reproducible inventory/audit script added by this plan is the maintained evidence
for the per-builtin details. As migration metadata becomes authoritative in
`BuiltinSpec`, the script must consume that metadata instead of retaining source-name
heuristics or a second semantic table.

## Target Architecture

`BuiltinSpec` remains the single PHP-facing declaration. It owns or points to shared,
typed descriptors for:

- canonical name, aliases, area, visibility, docs, and callable policy;
- signature, defaults, named arguments, variadics, by-reference parameters, and arity;
- argument validation and one authoritative argument/value-sensitive result resolver;
- precise conservative effects, warning/fatal/throw behavior, allocation, and
  argument mutation;
- result ownership, independence, borrowing, and argument aliasing;
- runtime and bridge requirements;
- a backend-neutral lowering strategy.

Lowering produces reusable EIR primitives or typed `RuntimeCall` instructions. A
runtime descriptor owns its stable identifier, logical signature, effects, result
ownership, requirements, target availability, and runtime symbol mapping. The generic
backend materializes that logical call through shared ABI helpers. Runtime/codegen
support may contain target-specific implementation details, but builtin declarations
and semantic lowering must not see registers, frame layout, assembly emitters, or raw
symbols.

Direct calls and demand-driven callable wrapper functions both invoke the same
backend-neutral semantic lowering. Compiler-resident exceptions are limited to real
language constructs or dedicated syntax and are represented explicitly, never as a
generic fallback.

## Migration Order and Gates

### 1. Reproducible inventory and structural audit

Create an audit that emits one record per builtin and rejects duplicate names,
missing semantic fields, missing runtime descriptors/target implementations, backend
imports below `src/builtins`, legacy signature/checker fallback, `BuiltinCall`,
per-name return/effects/requirements matching, callable exclusion allowlists, and
temporary fallback markers. Keep inventory output deterministic.

### 2. Registry semantics and typed runtime descriptors

Separate validation, result resolution, and lowering contracts. Extend the effects
and ownership vocabulary to cover every required observable behavior. Make checker,
AST optimizer, EIR lowering, ownership lowering, runtime requirement collection, and
docs consume these descriptors.

### 3. Backend-neutral lowering infrastructure

Provide normalized calls, typed operands/constants, block creation, primitive emission,
typed runtime-call emission, explicit requirements, source spans, and structured
lowering errors. Add EIR printer/validator support and a generic target-aware runtime
call materializer.

### 4. Family migration

Migrate each checklist family completely before marking it done. For every family:

1. compile cleanly;
2. run focused direct and callable tests;
3. inspect optimized and unoptimized EIR;
4. test checker/EIR type agreement;
5. test effects and ownership behavior;
6. confirm `src/builtins` has no target/backend dependency;
7. run focused supported-target checks when the path is target-sensitive.

An assembly compatibility adapter is allowed only while a family is actively being
migrated and must be removed before that family is checked off.

### 5. Callable and eval convergence

Generate callable wrappers as EIR on demand. Preserve case-insensitive and namespace
fallback lookup, callable arrays, aliases, `call_user_func`, `call_user_func_array`,
known-universe runtime dispatch, and eval/Magician parity. Encode non-dispatchable
semantics as registry metadata with an explicit reason.

### 6. Legacy removal

Delete the old opcode and dispatch only after all registry-backed calls have migrated.
Keep only explicitly enumerated language constructs such as `isset`, `unset`, `empty`,
`exit`, `die`, and dedicated `buffer_new` syntax where the AST requires lazy/l-value
semantics.

### 7. Final verification

Run the exact requested gates without `cargo fmt`:

```text
cargo build
cargo check --tests
focused registry/EIR/effects/ownership tests
focused codegen and error tests for every family
callable and dynamic-dispatch tests
optimizer tests with IR optimization on and off
cargo test
cargo build --example gen_builtins
python3 scripts/docs/extract_builtins.py --render --force
python3 scripts/docs/audit_builtins.py
python3 scripts/docs/elephc_builtins/validate_site_compat.py
git diff --check
```

Run `scripts/check_asm_comments.py` on every changed assembly-emitting file. Inspect
representative `--emit-ir` and `--emit-ir --no-ir-opt` output and prove that no opaque
builtin calls survive. Run focused Linux x86_64 and Linux AArch64 tests for
target-sensitive paths; state precisely which complete target suites remain delegated
to CI if local final matrix execution is impractical.

## Commit Structure

The intended local commit sequence is:

1. registry semantic model and reproducible inventory/audit;
2. backend-neutral EIR lowering and typed runtime calls;
3. simple/scalar builtin families;
4. complex/runtime/bridge builtin families;
5. callable, eval, and runtime-requirement convergence;
6. legacy-path removal and structural invariants;
7. tests, generated docs, architecture docs, and changelog.

No temporary compatibility commit is a valid final state.

## Completion Evidence

The migration completed on 2026-07-20 with the following final structural inventory:

- 469 registry-backed AOT builtins, 6 explicitly compiler-resident language or
  dedicated-syntax entries, and 4 eval-only names;
- 450 typed runtime-call targets, 18 EIR primitives, and 1 EIR graph lowering;
- 0 backend-dependent builtin home files, duplicate registry names, missing home
  files, or missing semantic descriptors;
- all three supported targets declared for every registry-backed semantic target.

Representative optimized and unoptimized EIR for `strlen`, `is_string`, and
`strtoupper` contained only `str_len`, `type_predicate`, and
`runtime_call runtime.string.to_upper`. Neither mode contained `BuiltinCall`,
`builtin_call`, or `language_construct_call`, and both produced identical output.

Final verification evidence:

- `cargo build`, `cargo check --tests`, and the third full `cargo test --quiet`
  completed successfully. The full run recorded 11,293 passed, 0 failed, and 37
  ignored tests across the workspace, including 6,638 codegen tests, 1,099 error
  tests, and 255 EIR backend smoke tests.
- Focused suites covered every builtin family and included 212 runtime-GC tests,
  398 callable tests, 578 object/OOP tests, 273 array tests, 255 optimizer tests,
  247 SPL tests, 236 regression tests, 186 string tests, and the complete registry
  and parity gates.
- The final `array_fill(null)`/Mixed append and `SplFileObject` CSV regressions passed
  on macOS AArch64, Linux AArch64, and Linux x86_64. Earlier ownership-sensitive
  typed-call regression filters also passed on both Linux targets. Complete Linux
  suites remain delegated to CI as required by the repository's local-test policy.
- Builtin docs regenerated 479 exported records and 936 pages; the builtin audit
  reported 0 errors and site compatibility validated all 953 generated pages.
- Every changed assembly-emitting file passed `scripts/check_asm_comments.py`;
  `scripts/audit_builtin_eir_boundary.py --enforce-target-architecture`, Python
  bytecode checks for the docs/audit tools, and `git diff --check` all passed.
- The initial migration used no `cargo fmt`, rebase, amend, push, or pull-request operation.

The branch was refreshed onto `origin/main` (`07391e965`) on 2026-07-21:

- all 11 migration commits were rebased and `origin/main` is an ancestor of the
  refreshed branch; no amend, push, or pull-request operation was performed;
- `main` added no builtin home files, so the inventory remained at 469 registry-backed
  AOT builtins; its changes to 12 array-callback builtins and `fseek()` were preserved
  inside the final semantic descriptors;
- `cargo build`, `cargo check --tests`, the new contextual-callback, typed-sort,
  `fseek(): int`, mixed-boundary, object-array covariance, and array-callback diagnostic
  regressions all passed;
- builtin generation remained stable at 479 exported records and 936 pages, with 0
  audit errors and all 953 generated pages passing site compatibility;
- the enforced builtin-boundary audit remained unchanged at 450 runtime calls, 18 EIR
  primitives, 1 EIR graph, and 0 structural errors.

The thematic local implementation commits are:

1. `2a8f2e896` — inventory and structural audit;
2. `0e26e9762` — typed EIR targets;
3. `542759aef` — shared builtin semantics;
4. `20368fa9e` — helper-centric runtime calls;
5. `9e0301c9a` — descriptor-derived callable wrappers;
6. `c68a51e64` — registry-owned argument lowering;
7. `c97c035a5` — final legacy-boundary enforcement;
8. `1d52a0c9f` — EIR result and ownership contracts;
9. `095bb42e5` — final typed result-storage regressions;
10. `b43c68957` — documentation and completion evidence;
11. `6bf80798a` — generated-document ending normalization.
