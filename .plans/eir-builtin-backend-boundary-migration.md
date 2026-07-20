# Plan: Backend-Neutral Builtin to EIR Boundary

## Checklist

- [ ] Keep a reproducible per-builtin inventory covering the registry, signatures,
  validation, checker and EIR result types, effects, ownership/aliasing, lowering,
  runtime helpers, bridge requirements, callable/eval support, targets, and tests.
- [ ] Introduce one registry-owned semantic model for validation, result typing,
  effects, ownership/aliasing, requirements, callable policy, and lowering strategy.
- [ ] Introduce stable typed runtime function identifiers and central runtime
  descriptors with logical ABI, effects, ownership, requirements, and target support.
- [ ] Add a backend-neutral builtin lowering context that can emit EIR primitives,
  control flow, and typed runtime calls without importing codegen concepts.
- [ ] Migrate scalar predicates and conversions.
- [ ] Migrate math builtins.
- [ ] Migrate string builtins.
- [ ] Migrate array builtins.
- [ ] Migrate callbacks and sorting builtins.
- [ ] Migrate object, class, and reflection builtins.
- [ ] Migrate date and time builtins.
- [ ] Migrate JSON, serialization, regex, and hash builtins.
- [ ] Migrate filesystem, stream, and I/O builtins.
- [ ] Migrate process, system, and environment builtins.
- [ ] Migrate bridge-backed builtins.
- [ ] Migrate pointer, buffer, and elephc extension builtins.
- [ ] Migrate internal and prelude builtins.
- [ ] Unify direct, first-class, dynamic string, callable-array,
  `call_user_func*`, and eval/Magician callable paths around the same semantics.
- [ ] Remove `BuiltinSpec` assembly hooks and every `src/builtins` dependency on
  `src/codegen`.
- [ ] Remove registry-backed `Op::BuiltinCall`, the separate EIR return-type chain,
  legacy signature/checker fallback, duplicate effects and requirements matching,
  and separate callable assembly wrappers.
- [ ] Add structural invariants plus registry, EIR, optimizer, ownership, callable,
  runtime/linking, and supported-target regression coverage.
- [ ] Update contributor and internal architecture documentation, regenerate builtin
  documentation, and add the user-facing changelog entry.
- [ ] Run all requested build, test, docs, assembly-comment, EIR, target, and diff
  gates; audit every objective requirement against current evidence.
- [ ] Package the completed, green migration as thematic local commits without
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
