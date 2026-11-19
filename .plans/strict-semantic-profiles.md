# Strict semantic profiles

- [x] Inspect CLI, source profiles, checking, folding, EIR, runtime, eval, and caches.
- [x] Record the agreed umbrella, PHP-only naming, and exact string-key rules.
- [x] Adopt the detailed boundary defaults from the approved implementation plan.
- [x] Add the shared semantic contract and resolve CLI requests once.
- [x] Rename the existing PHP-only option and its maintained integration surfaces.
- [ ] Propagate the immutable profile through compiler phases and runtime contexts.
- [x] Complete strict-dynamic-code source, execution, and REPL boundaries.
- [x] Implement numeric rules, checked conversions, and catchable integer overflow.
- [x] Implement stable bindings, boolean conditions, comparisons, and string rules.
- [ ] Implement nullability, mixed/union narrowing, and initialization checks.
- [ ] Implement exact array keys, element contracts, and checked reads.
- [ ] Complete builtin, callable, reference, eval, and host-boundary parity.
- [ ] Complete cache partitioning, optimizer guards, focused tests, and examples.
- [ ] Publish the complete umbrella only when every advertised guarantee is enforced.

Analysis baseline: `cdab890c65048707e6c5f3220d5077b78b5ab49b` on
`feat/strict-mode`. This document retains the approved contract and tracks implementation.
The original analysis changed only this plan; verified code work is recorded below.

## Agreed behavior

Each semantic guarantee has an independently selectable `--strict-*` option.
`--strict` enables the entire registered family. A specific positive or negative
option overrides the umbrella regardless of argument order; repeated explicit
options for the same feature use last-wins ordering. Resolving options produces
one immutable profile, not downstream argument strings or scattered booleans.

Rename `--strict-php` to `--php-only`, with no legacy CLI alias. Preserve the
existing physical-source eligibility: PHP source is restricted, while `.lfc`
and compiler-generated syntax retain their current extension support. This
syntax policy is separate from semantic strictness. Semantic strictness applies
to user PHP and LFC code, and is never disabled by `declare(strict_types=0)`.

With `--strict-array-keys`, strings retain their exact bytes and string kind:
`"1"`, `1`, and `"01"` are three distinct keys. Only integer and string keys are
accepted. Hash collisions still compare key kind and original value; hashes are
not substituted for keys. With no new semantic flags, preserve existing behavior.

## Feature catalog and ownership

The following catalog includes the already existing `--strict-opcache`.
Independence means no feature silently enables another. Restrictions can overlap:
disabling one feature does not cancel restrictions imposed by another feature.

| Option | Owning rule and main integration points |
|---|---|
| `--strict-locals` | Stable binding contracts, including numeric changes. Extend `src/types/checker/stmt_check/assignments/locals.rs`, `mixed_storage_scan.rs`, `numeric_regions.rs`, and binding/loop joins. |
| `--strict-coercions` | Reject implicit PHP value conversions at arithmetic and assignment/call boundaries. Extend `src/types/param_binding.rs`, its union helpers, checker compatibility/call validation, EIR argument and return coercion. |
| `--strict-overflow` | Check integer literals and arithmetic, including unary negation, updates, compound stores, powers, and `intdiv` edge cases. Touch lexer literal provenance, arithmetic inference, AST/EIR folding, EIR operations, and target emitters. |
| `--strict-arithmetic` | Stable operator signatures and result kinds: `/` returns float, `%` and bitwise operations require integers. Integer power with negative exponent requires an explicit floating-point operation. |
| `--strict-casts` | Explicit conversions have checked domain/range behavior. Extend `src/optimize/fold/casts.rs`, `src/ir_lower/expr/ternary_cast.rs`, and `src/codegen/lower_inst/numeric_cast.rs` plus boxed runtime casts. |
| `--strict-bools` | Boolean conditions and logical operands, including ternaries, loop conditions, short-circuit expressions, xor, and callback-result conditions. Reuse checker condition traversal and `src/ir_lower/expr/branch_conditions.rs`. |
| `--strict-comparisons` | Reject loose equality syntax; require noncoercive ordering, switch matching, and builtin comparison modes. Extend checker inference, comparison lowering, switch lowering, and relevant builtin semantic descriptors. |
| `--strict-nullability` | Require a proven non-null view for operations that require one. Preserve explicit null tests, nullsafe access, `isset`, and `??`. Reuse existing narrowing and member-receiver facts. |
| `--strict-mixed` | Require narrowing when a mixed/union value reaches an operation unsupported by all alternatives. Keep explicit mixed containers and valid union operations. Cover direct calls, descriptors, reflection, eval re-entry, and references. |
| `--strict-strings` | String concatenation requires strings; formatting/output have explicitly documented conversion contracts. Cover compound concatenation, object string hooks, AOT and eval. |
| `--strict-array-types` | Pin inferred or declared element/key contracts; heterogeneous data uses an explicit mixed/union contract. Reuse typed arrays and cover builtin mutation, aliases, and reference write-back. |
| `--strict-array-keys` | Exact typed keys; forbid bool/float/null coercion. Extend `src/types/array_keys.rs`, all key/shape planners, EIR materialization, and runtime array producers. |
| `--strict-initialization` | Require initialized local/property reads. Reuse binding-flow and initialized-property machinery; add guards where static proof is unavailable. |
| `--strict-indexing` | Missing keys and out-of-range reads throw; probing constructs retain their intentional missing-value behavior. Cover packed/hash/boxed reads and string offsets. |
| `--strict-dynamic-code` | Refuse runtime-provided source; statically verified literal eval and known includes remain possible. Audit resolver/autoload, literal-eval plans and their bridge fallbacks, runtime includes, REPL, and test-generated hosts. |
| `--strict-opcache` | Preserve the existing exception for forced invalidation of AOT-frozen code; move its option into the shared catalog and umbrella. Its current lowering is in `src/opcache_prelude/build.rs`. |

## Findings that determine the design

1. `src/pipeline.rs` folds constants before checking. `src/optimize/fold/ops.rs`
   already promotes overflowing constants and folds PHP coercions. Validation
   only inside the checker would miss source operations erased by folding.
2. `src/lexer/literals/numbers.rs` converts overflowing integer spellings into
   float tokens. Once that happens, a checker cannot distinguish an overflowing
   integer spelling from a deliberate float literal.
3. `CheckOptions` currently carries only `strict_locals`. PHP-only visibility
   instead uses thread-local state in `src/php_only.rs`, while PHP version/SAPI
   live in `src/codegen_support/compilation_context.rs`. Repeating this split for
   sixteen features would produce inconsistent consumers.
4. `src/types/param_binding.rs` already shares argument-binding classification
   between checking and lowering. `src/types/call_args/` owns argument order and
   matching. Extend these boundaries rather than introducing another planner.
5. `src/types/array_keys.rs` changes a literal `"1"` to integer key type, and an
   unknown string to mixed key type. Its storage-shape helpers also assume PHP
   normalization, so replacing one runtime call is insufficient.
6. `src/codegen/lower_inst/hashes.rs` already has `HashKeyNormalization` with
   `Php`, `PhpAlreadyDiagnosed`, and `RawString`. Descriptor argument containers
   use `RawString` because parameter names must remain strings. That mode alone
   does not implement strict validation of other key kinds.
7. `hash_key_hash.rs`, `hash_key_eq.rs`, `hash_get.rs`, and `hash_set.rs` already
   support distinct integer/string key representations and collision checks.
   Exact keys do not inherently require a new hash algorithm or header layout.
8. Runtime constructors and array builtins normalize keys independently:
   `array_fill_keys`, `array_combine`, `array_flip`, `array_count_values`, spreads,
   and associative JSON decoding are among the affected paths.
9. `ICheckedAdd/Sub/Mul/Pow` implement PHP promotion. Existing `*ToInt` sink
   specializations can report a fatal overflow, while fused chains preserve PHP
   floating-point continuation. Neither is automatically a strict throwing op.
10. Literal-eval names/plans and `Immediate::ProfiledData` currently carry a
    PHP-only boolean. Magician has both a walker and bytecode VM. Its executable
    cache currently keys modules and compilation gaps by source bytes alone.
11. Runtime object identity includes PHP version and runtime features, while REPL
    identity includes raw compile arguments. Both need an explicit decision
    about effective semantic-profile identity when generated behavior changes.

## Shared architecture

### Dependency-neutral semantic crate

Add `crates/elephc-semantics/` as a small rlib consumed by the compiler, Magician,
and the builtin contract where metadata requires it. It is not a bridge staticlib
and introduces no user-program link requirement by itself.

Suggested cohesive modules:

```text
crates/elephc-semantics/src/
  lib.rs           exports and module documentation
  profile.rs       StrictFeature, SemanticProfile, stable encoding/version
  catalog.rs       feature names, help text, ALL, and option resolution metadata
  conversions.rs   conversion-site and scalar-conversion decisions
  numeric.rs       arithmetic signatures and checked scalar evaluation
  comparisons.rs   scalar comparison policy
  array_keys.rs    PHP-normalized versus exact typed key classification
  violations.rs    typed semantic violation kinds
```

The crate has no AST, checker, EIR, assembly, target register, or runtime-cell
dependencies. Share small scalar categories and pure decisions; do not clone the
compiler's complete `PhpType` hierarchy into a second type system.

Keep compiler-specific adapters in a focused `src/semantics/` module:

- `context.rs`: compose semantic flags with syntax/source policy and PHP's
  `strict_types` call-site directive without conflating the three.
- `validation.rs`: source-level invariants that must survive pre-check folding.
  Reuse an authoritative AST traversal where possible; do not duplicate checking.
- `conversions.rs`: translate full PHP types and semantic sites into a plan that
  both checking and lowering consume, building on `types::param_binding`.
- `diagnostics.rs`: attach source spans, flag names, and existing compiler errors
  to neutral violation kinds.

Existing subsystem helpers remain authoritative adapters. In particular,
`types::array_keys` and `types::param_binding` should delegate relevant policy
decisions instead of acquiring parallel implementations in a new utility bucket.

### Explicit profiles and plans

Resolve CLI requests into `SemanticProfile` once. Carry it through `CheckOptions`,
`Checker`, `CheckResult`, EIR `Module`, lowering contexts, backend/runtime emission
inputs, and eval contexts. Test-only/default entry points must explicitly select
the existing permissive profile. Do not make the backend infer semantics from
command-line strings, the presence of boxed storage, or a mutable global switch.

Retain physical source metadata for PHP-only eligibility and `strict_types`.
Expand profiled eval metadata to carry the effective semantic profile. Profile
identity must survive cloning, inlining, generated thunks, callback descriptors,
and AOT/eval re-entry. A narrowly scoped RAII compatibility adapter may bridge an
existing thread-local consumer during migration; it must not become a second
authoritative store or leak between nested/parallel compilations.

Useful interfaces, with final names chosen during implementation:

```text
resolve_strict_requests(requests) -> SemanticProfile
plan_conversion(profile, site, source, destination) -> ConversionPlan
plan_numeric_operation(profile, operation, left, right) -> NumericPlan
classify_array_key(profile, source_kind, optional_literal) -> KeyPlan
require_boolean(profile, actual_type) -> Result
require_usable_value(profile, actual_type, operation) -> Result
```

Conversion sites distinguish implicit value conversion, explicit cast, condition,
concatenation, array key, output formatting, and representation-only adaptation.
Boxing/unboxing, retaining an owner, packed-to-hash storage conversion, and the
floating implementation of an explicitly floating-result operator remain legal
when they do not change the promised PHP-visible value contract.

Store authoritative checked plans/facts on `CheckResult`, keyed by body and span
where necessary, using the existing member-receiver/builtin-call metadata pattern.
Lowering consumes these plans rather than independently guessing acceptance.

## Phase integration and reusable mechanisms

### Frontend, literals, and optimization

- Preserve integer-spelling provenance until the parser can resolve signed
  magnitude, including the minimum signed integer and radix spellings. Avoid
  classifying an intentional float literal as integer overflow. Audit both lexer
  implementations when runtime source parsing remains available.
- Enforce source-level restrictions before destructive folding. Pass the profile
  to every folding entry point, including declaration defaults, generics, autoload,
  builtin folds, and literal eval. A helper that only returns `Option<literal>`
  must not quietly turn a strict violation into a successful PHP fold.
- Use typed fold outcomes such as value, not-foldable, or semantic violation.
  Keep source diagnostics stable across optimization settings. Do not disable all
  optimization merely because a strict feature is enabled.
- Integrate AST effects and EIR effects. A discarded strict operation that can
  throw remains observable. Existing `checked_int_sink`, checked-chain fusion,
  range analysis, identity folding, and strength reduction must preserve the
  selected failure behavior and source evaluation order.

### Checking and binding flow

- Reuse local binding origins and loop/branch joins. Stable binding contracts
  differ from temporary narrowed views: assigning another permitted union member
  is legal, while replacing an inferred integer binding with a float is not.
- With strict-locals enabled alone, PHP arithmetic can still produce a float on
  overflow. The assignment boundary must prove the original binding contract or
  check it before storing; it must not silently enable strict-overflow or widen
  the binding. Apply the same rule to updates and externally written references.
- Preserve legitimate `unset`-then-new-binding behavior. Cover parameters,
  captures, globals, by-reference locals, variadics, and foreach aliases.
- Reuse existing narrowing and `definitely_bound_locals`; extend proofs rather
  than adding an unrelated flow engine. Invalidate facts across mutable aliases,
  callbacks, property writes, and eval exactly where existing effects demand it.
- Empty arrays may infer their contract at the first informative write. Explicit
  mixed/union contracts remain usable. Record type origin where needed so an
  accidental inferred widening does not count as an explicit mixed declaration.

### EIR and native failure handling

Use typed operations/plans for checked value behavior. For example, integer
`IAddOrThrow`, `ISubOrThrow`, `IMulOrThrow`, `INegOrThrow`, and `IPowOrThrow`
produce integer results and differ explicitly from PHP-promoting operations.
Checked casts and ordinary-versus-probe array reads need equivalent typed metadata.
Update builder, printer, verifier, ownership/effects, and optimizer consumers.

Extract architecture-aware overflow detection from the existing checked lowering
and share it with the new operations. Use the existing exception/unwind machinery,
not the current narrowed-sink fatal message. Proposed runtime failure classes:
`ArithmeticError` for integer overflow, `TypeError` for invalid value kinds,
`ValueError` for cast domain/range failures, `OutOfBoundsException` for missing
ordinary element reads, and existing `Error` behavior for uninitialized properties.
All these classes already exist in the compiler catalog.

Every throwing path must release owned temporaries and preserve borrowed inputs,
reference cells, COW, same-frame catches, finally blocks, and host error boundaries.
Implement both AArch64 and x86_64 through shared ABI helpers. Device and Simulator
iOS targets require their normal compile/emitter coverage in the same change.

### Array key policy

Promote the semantic part of `HashKeyNormalization` into an explicit shared
`ArrayKeyPolicy`/key plan. Keep diagnostic suppression and register materialization
as backend details. Descriptor parameter names remain an explicitly raw key space;
ordinary PHP arrays choose normalized or exact keys from the semantic profile.

Apply the same decision to literal inference, dynamic string inference, packed/hash
shape choice, reads, writes, nested property writes, references, silent probes,
unset, and append-history bookkeeping. A string key such as `"0"` requires hash
storage in exact mode and does not advance the automatic integer index.

Audit all key-producing/transforming paths: union, merge, spreads, array flip,
fill/combine/count/column, iteration, serialization, associative JSON decode,
array casts, superglobals, and eval/runtime callbacks. Preserve actual key kind
when unpacking calls: exact string keys are named arguments, not positions.

Keep low-level hash primitives operating on already-classified typed keys.
Use profile-aware user-array wrappers instead of globally changing the existing
PHP-normalization primitive that other internal domains may intentionally use.
Do not add a per-array header field without evidence that a supported boundary
must mix policies inside one runtime. Cross-artifact and Zend conversion boundaries
must validate or explicitly convert incompatible key spaces, never lose entries
silently when `"1"` and `1` collapse into a PHP array.

### Builtins, generated source, eval, and caches

- Add effective profile input to `BuiltinSemanticInput` and lowering contexts.
  Keep comparison/key/conversion policies in shared semantic descriptors and, when
  eval needs the same metadata, the neutral builtin contract. Do not maintain a
  second blacklist of builtin names or change standard defaults invisibly.
- Reuse `CallArgPlan` for source order, named/spread matching, and ref-like storage.
  Apply the same binding plan to direct calls, first-class callables, aliases,
  descriptor invokers, methods/constructors, externs, and reference write-back.
- Compiler-generated preludes require an explicit audited boundary. Existing
  `SourceMode::Internal` exempts syntax from PHP-only checks; it must not silently
  exempt public values/results from strict guarantees. Internal implementation
  operations can retain reviewed explicit semantics behind checked boundaries.
- Magician needs the same profile on `ElephcEvalContext`, native context setup,
  walker expression/statement handling, `RuntimeValueOps`, bytecode compile/VM,
  builtin adapters, and native re-entry. A literal eval that cannot satisfy the
  selected rules must not quietly fall back to a permissive interpreter.
- `--strict-dynamic-code` may reject opaque source while other strict features
  still have to work with dynamic source when that one flag is disabled. The
  boxed eval scope cannot remain an exemption from stable local/array contracts.
- Include effective profile and semantic ABI version wherever cached output or
  acceptance depends on them: runtime objects, literal-eval names/plans, shared
  descriptor tables, executable and negative-gap caches, and REPL hosts. Parsed
  source caches can remain shared only if they retain source evidence and rerun
  profile validation; otherwise partition them too. Update format guards for any
  changed serialized shape. Runtime build fingerprints must include the new crate.
- Audit executable, web worker/request, REPL, test runner, cdylib/staticlib, hosted
  extensions, and phpext boundaries. An unsupported profile/ABI crossing needs an
  explicit diagnostic; exported artifacts must not advertise an unenforced mode.

## Implementation boundary defaults

- Keep IEEE 754 floats, including rounding, NaN, and infinity. Strict casts to int
  reject nonfinite/out-of-range input. Explicit finite float-to-int conversion
  truncates toward zero; valid full numeric strings may be parsed, invalid or
  trailing-junk strings fail. Rounding is not an implicit type conversion.
- String ordering under strict comparisons is bytewise, including numeric-looking
  strings. Equality syntax uses `===`/`!==`; switch uses exact matching and rejects
  statically incompatible case types. Comparison builtins must select an explicitly
  noncoercive mode, rather than silently rewriting their standard arguments.
- Concatenation requires string values. Echo/print/interpolation are explicit
  formatting contexts with reviewed scalar and Stringable rules; arrays do not
  become the text `Array` implicitly.
- Known invalid types/literals are compile diagnostics. Value-dependent failures
  throw during execution. Specify diagnostic behavior for invalid constants in
  unreachable code consistently before adding fold regressions.
- Missing ordinary array/string reads throw; `isset`, `empty`, null coalescing,
  and nullsafe access retain their explicit probing/absence semantics.
- Static literal eval/includes are checked with the same profile; opaque eval and
  runtime include are rejected by strict-dynamic-code. REPL full-strict operation
  must report that conflict or require its explicit override. Autoload/injection
  that resolves source at compilation time remains possible.
- `--php-only` retains current scope rather than promising reference-PHP semantics
  when combined with opt-in strict semantic changes. Document this distinction.

## Delivery order

1. Shared catalog/profile, request resolution, and the PHP-only rename. Include
   `strict-opcache` and existing strict-locals in the catalog. Update all CLI
   entry points, diagnostics, tests, generated docs inputs, and maintained docs.
2. Explicit propagation, source evidence, conversion plans, failure ABI, and cache
   identities. Preserve permissive controls; establish the test helper below.
3. Numeric guarantees and stable binding contracts. Share folding and runtime
   failure decisions; complete target codegen and optimizer coverage per feature.
4. Boolean/comparison/string rules, nullability/mixed checks, and initialization.
5. Exact keys, typed arrays, and indexing, including runtime builtin producers,
   COW/reference behavior, and external format boundaries.
6. Complete remaining eval/callable/prelude/host integration and user docs/examples;
   expose the finished umbrella. Earlier PRs may expose completed individual
   features, but must not accept unimplemented flags as successful no-ops.

Each implementation PR must include the affected alternate paths at once. Eval,
supported targets, and optimizer-off behavior are completion criteria for every
feature, not a deferred port after enabling the flag.

## Verification and completion criteria

Add reusable strict fixture support under `tests/codegen/support/`: take an
effective profile and expected compile diagnostic, runtime output/exception, or
IR property. Reuse existing compiler/runner/heap helpers. Do not bundle cases
that require different flags, separate declarations, or different exit behavior.

Test resolution and pure policies without native codegen: umbrella expansion,
explicit override precedence in both orders, per-feature defaults, existing
strict-opcache membership, PHP-only separation, typed key classification, and
scalar boundary tables. Cover important feature pairs without compiling the
entire power set of profiles.

Per feature, add permissive controls, strict rejection/acceptance, runtime-unknown
inputs, IR-opt on/off, direct/FCC/descriptor calls, aliases and references, AOT
literal eval and walker/bytecode parity when applicable, cache hit/miss behavior,
and a readable example. Numeric tests include MIN/MAX, unary MIN, zero divisors,
powers, post/pre updates, and checks caught by finally/destructors. Key tests
include `"0"`, `0`, `"01"`, `"-0"`, empty strings, embedded NUL, same-byte keys
from different owners, collisions, append history, and every producing builtin
family. Add focused runtime-GC tests for exception cleanup and key ownership.

Follow the machine-wide build queue for every compiling/linking/heavy test
command. Use exact new/directly changed test names and the narrowest test binary;
count matches before running. Run only relevant local checks and rely on CI for
the full supported-target matrix. Regenerate/audit builtin docs when their
descriptors or contracts change. Update CLI/types/arrays/internals documentation,
README, and examples. Leave CHANGELOG release handling to its dedicated workflow.

The analysis-only baseline needed no Rust validation. Verified implementation
evidence and outstanding guarantees are recorded below.

## Implementation progress

Verified through step216 (this commit). Remaining feature integration is active. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask contains Locals, Bools, DynamicCode, Opcache,
Arithmetic, Overflow, Strings, Coercions, Casts, Comparisons, ArrayKeys,
Initialization and Indexing. Nullability, Mixed and ArrayTypes remain gated, as does
the complete umbrella, until their whole contracts are enforced. The verified
steps below retain the source, reference, eval, ownership and target evidence.

This progress ledger condenses verified implementation through step133.
Detailed evidence remains in Git history, historical Code Journal plan revisions,
the linked work logs, and the current agent-facing architecture document
3739892b. The approved behavior and delivery requirements above are unchanged.

### Verified foundations and numeric boundaries

The [foundation record](strict-semantic-profile-foundations.md) retains the
implementation details and evidence for the early profile, numeric, declaration,
cast and native eval boundaries. The active architecture map is Code Journal
document3739892b; the complete signed commit ledger stays below.

- Shared ABI1 profiles own sixteen independent guarantees, source-aware frontend,
  EIR/runtime/eval propagation, cache identity and PHP-only separation. DynamicCode,
  Bools, Arithmetic and Overflow have verified public paths on every target.
- Numeric operations retain original source evidence, independent operand/result/
  overflow policies, native object adapters and balanced protected callbacks.
  Exact key classification is groundwork, while runtime key/type/indexing
  completion remains outstanding.
- Exact scalar, nominal and structural declarations, defaults, returns, locals
  and descriptors share conversion plans. Producer expressions, physical policies
  and explicit binding metadata survive copies and folding; legal PHP widening
  remains separate from Coercions.
- Checked explicit conversions share exact decimal/radix kernels and original
  native projections. Direct, callable, reference and eval paths preserve owners,
  catchable failures and write-before-publication rules. Remaining cast/reference/
  host admission audits are recorded in the current sections below.
- Protected native hooks contain PHP exceptions beneath Rust, use explicit status
  and Throwable ownership, and preserve output slots on failure. All supported
  targets share these contracts; native and emitter evidence remain required.

### Completion work, in order

1. Strengthen Locals across numeric binding changes, parameters, captures, globals,
   by-reference writes, foreach aliases, loop joins, unset/rebind and eval scopes.
   Locals alone must not enable Overflow. Step115 repairs the permissive catch-name
   collision using the native slot occupant before publishing its contract.
2. Complete Nullability, Mixed and Initialization through source checking, EIR,
   runtime and all public hosts. Preserve intentional probes and explicit narrowing.
3. Finish exact keys in every read/write/probe/unset/append/spread and producer,
   including JSON/casts/serialization. Pin array contracts and mutators; ordinary
   missing/out-of-range reads throw while probing keeps missing-value semantics.
   Preserve COW and reference ownership across every failure.
4. Audit executable/web/REPL/staticlib/cdylib/phpext/eval/callback boundaries,
   effective cache identity and independent/combined flags for the complete
   profile. Update docs, README, examples and generated metadata. Enable and
   verify the full umbrella only after every registered guarantee is enforced.

All five supported targets remain first-class. Local native verification has run
on Linux x86_64; library/runtime emitter checks cover the complete target matrix.
CI provides executable coverage on the remaining supported hosts and iOS linking.
The focused checks for each verified commit passed with both optimizer settings
where relevant, plus ownership/heap evidence for new failure paths. No full local
suite, push or PR has been requested or performed.

### Current implementation map and open admission audits

The [detailed admission history](strict-semantic-profile-admissions.md) retains
all former cast, declaration/reference, builtin, comparison and string evidence
through109. It complements the numeric foundation record and current Code
Journal architecture document3739892b. Reuse the established mechanisms:

- Neutral value/conversion plans and full declaration projections feed checking,
  EIR, native reference publishers and both eval engines. Producer source policy
  remains separate from a consumer's physical mode; prepared invocations retain
  their original argument and result contracts.
- Native stores, initialization, references and static publication validate a
  rooted candidate before publication and preserve cleanup/Throwable ownership.
  Eval-owned property sources remain live through aliases and native calls.
- Comparisons is admitted after source, native value-class, callable/Reflection,
  native/eval/REPL and five-target library checks. Argument descriptors own raw
  scalar/string boxes across free-function and method callback failures.
- Strings is admitted after source/native/eval/Reflection-callable-independent
  formatting checks. Native XML writes inspect the original selection before
  mutation. Logical string null survives boxing and predicates. Native activation
  snapshots share managed reference identity and retire copied leases after
  writeback; global callback scope stays materialized.

Coercions and Casts are publicly admitted after signed110 and111. The completed
[reference/cast audit](strict-semantic-profile-references.md) retains their
Reflection, native/eval reference, callback and lifetime proofs. Comparator owner
and native return/effect foundations are signed112 and113; public Comparisons
admission is verified114. Its [detailed history](strict-semantic-profile-comparisons.md)
retains the adapter, reference-storage, dispatch and method-owner corrections. Step115
repairs the original permissive native/eval catch-name collision. Stronger Locals
still needs interpreted parameter, capture, global, foreach, reference reentry and flow coverage. Nullability/Mixed/Initialization, exact keys/
types/indexing and full host/cache/combination acceptance remain unfinished.

### Verification discipline

Count exact test registrations, use narrow package/test-binary filters, and queue
all compiling/linking/heavy test commands through me-build-lock. Freeze source
before verification: the compiler build fingerprint hashes every source file,
including tests. New Rust files need Purpose/Called from/Key details headers and
every function needs a docblock. Do not run cargo fmt or edit CHANGELOG.

Builtin homes and eval bindings require the update-builtin-docs workflow. Build
the curl exporter through the queue, then set ELEPHC_DOCS_PREBUILT=1 and
PYTHONDONTWRITEBYTECODE=1 for every generator and audit, including the architecture
boundary gate, to prevent hidden Cargo invocations. Check every modified assembly
comment, example syntax and git diff --check. Remove only task-owned temporary
paths. Keep this plan, agent documentation and the current journal log synchronized.

### Verified commit ledger

| Step | Commit | Change |
|---|---|---|
| 1 | `09ab0ce4af` | feat(strict): add semantic profiles and separate php-only mode |
| 2 | `aa19924cdf` | feat(strict): enforce boolean conditions and callback results |
| 3 | `9c81eca471` | feat(strict): preserve checked integer literal magnitudes |
| 4 | `fa3c16c2c9` | feat(strict): isolate parsed source caches by semantic profile |
| 5 | `d3a2f78e07` | feat(strict): retain eval source profiles through parsing and AOT |
| 6 | `4d090bd18b` | feat(strict): validate native runtime semantic identity |
| 7 | `3edd7edddb` | feat(strict): lower checked scalar arithmetic through shared guards |
| 8 | `614987b9f4` | feat(strict): preserve checked boxed arithmetic and power policies |
| 9 | `ff251eda10` | fix(strict): preserve checked arithmetic through AST optimization |
| 10 | `9b0a136df5` | feat(strict): reject proven source integer overflow |
| 11 | `8f60c8398a` | feat(strict): check integer updates before storage mutation |
| 12 | `4bc4f9441d` | feat(strict): retain builtin invocation profiles across callable paths |
| 13 | `894e7cfed4` | feat(strict): check absolute values through builtin call contracts |
| 14 | `2449202cbc` | feat(strict): share checked power graphs across builtin callables |
| 15 | `e3cf818471` | feat(strict): preserve exact numeric power operands and callback effects |
| 16 | `317821d610` | feat(strict): check numeric array reductions and callback cleanup |
| 17 | `2e3940f361` | feat(strict): validate numeric reductions and integer division boundaries |
| 18 | `6e0cc17eb7` | feat(strict): preserve typed literal keys and append provenance |
| 19 | `872d1c3749` | feat(strict): contain checked numeric eval callbacks |
| 20 | `e67790a9a7` | feat(strict): share exact checked numeric operand kernels |
| 21 | `74267d737e` | feat(strict): check eval numeric array reductions |
| 22 | `b911cc2942` | feat(strict): check eval updates before mutation |
| 23 | `8d8a8a3b08` | feat(strict): contain eval warning handler exceptions |
| 24 | `4aeafcb3b6` | feat(strict): protect eval division and share operand snapshots |
| 25 | `83b3d69229` | fix(strict): preserve numeric warning and kind error order |
| 26 | `035e8701ca` | feat(strict): protect eval modulo conversions and ownership |
| 27 | `1c951eb099` | feat(strict): carry checked division and modulo through eir |
| 28 | `839f071ddb` | feat(strict): share numeric operand domains across compiler and eval |
| 29 | `c1927fba57` | fix(strict): preserve catchable intdiv failures across eval calls |
| 30 | `88de5f36f5` | feat(strict): share exact integer power results with eval |
| 31 | `b94b9ccfa8` | feat(strict): guard unary operands and updates before writeback |
| 32 | `e0f29e32e1` | fix(strict): preserve ieee signs in scalar floating negation |
| 33 | `1b59635ae4` | feat(strict): preserve exact unary values across protected eval hooks |
| 34 | `22de3cd29b` | feat(strict): preserve unary numeric contracts through ast and eir |
| 35 | `534ce59174` | feat(strict): apply independent numeric policies to builtin power |
| 36 | `3d27b323d5` | fix(strict): preserve caller policies in outlined operators |
| 37 | `266dd4c139` | feat(strict): guard original numeric builtin parameter kinds |
| 38 | `3413afbaeb` | feat(strict): enforce numeric array reduction domains |
| 39 | `113f7497ec` | fix(strict): reject invalid checked aggregate callback sources |
| 40 | `e78091ef0c` | feat(strict): guard original scalar parameter contracts |
| 41 | `6ba184fab7` | feat(strict): preserve declared scalar return contracts |
| 42 | `4fcbe3e803` | feat(strict): preserve inferred scalar return alternatives |
| 43 | `6d06bfe2db` | feat(strict): carry checked conditional closure result contracts |
| 44 | `efcfed164c` | feat(strict): enforce declared scalar initializer contracts |
| 45 | `83b2501b36` | feat(strict): retain declared contracts across local stores |
| 46 | `de9ba02ca7` | feat(strict): validate original scalar declaration defaults |
| 47 | `9c9cbe461a` | feat(strict): preserve declaration kinds and union failures |
| 48 | `6258e85218` | feat(strict): materialize declared scalar constant literals |
| 49 | `beac107cab` | feat(strict): bind omitted parameter defaults before invocation |
| 50 | `87252a0179` | feat(strict): guard public descriptor scalar arguments through eir |
| 51 | `ec3e9e0feb` | feat(strict): preserve exact scalar and nominal union contracts |
| 52 | `39f10226af` | feat(strict): enforce structural union and callable value contracts |
| 53 | `6cfe328f09` | feat(strict): validate original source call arguments |
| 54 | `82c251fd4d` | feat(strict): retain expression producer source profiles |
| 55 | `68d8803dfb` | feat(strict): bind nonliteral declaration producers before consumption |
| 56 | `ea9e76218c` | feat(strict): bind property default producers before initialization |
| 57 | `a74801937f` | feat(strict): preserve global constant producer expressions |
| 58 | `58ea161e3b` | feat(strict): share checked numeric cast source contracts |
| 59 | `ae8b7a0f8c` | feat(strict): lower checked native scalar casts |
| 60 | `72136d03e2` | feat(strict): enforce checked numeric casts in eval |
| 61 | `31ffb78d5b` | feat(strict): share exact checked integer radix kernels |
| 62 | `7977298569` | feat(strict): share checked builtin numeric cast contracts |
| 63 | `98ae35e13c` | feat(strict): check settype before reference writeback |
| 64 | `188304e3f4` | feat(strict): validate original native object cast values |
| 65 | `309bdac54b` | feat(strict): check broad native object cast domains |
| 66 | `6c74bdaaed` | feat(strict): check public gmp integer projections |
| 67 | `2802808721` | feat(strict): prove original gmp integer cast sources |
| 68 | `95f5d407ec` | feat(strict): share settype target validation |
| 69 | `05cef3fb07` | feat(strict): prepare checked settype callable conversions |
| 70 | `222c4281fb` | feat(strict): complete settype collection conversions |
| 71 | `c064a2192f` | feat(strict): share scalar builtin conversion graphs |
| 72 | `c7361405c6` | feat(strict): preserve contextual cast warning ownership |
| 73 | `0146784a0b` | feat(strict): preserve live builtin reference places |
| 74 | `180fbf3a6c` | feat(strict): retain erased reference receiver bindings |
| 75 | `1514409912` | feat(strict): preserve runtime property reference names |
| 76 | `0c94688cfc` | feat(strict): bind known dynamic property arguments |
| 77 | `fed394dc1d` | feat(strict): canonicalize dynamic mixed property references |
| 78 | `c694b9c796` | feat(strict): enable the arithmetic semantic profile |
| 79 | `a1a4296e52` | feat(strict): enable the overflow semantic profile |
| 80 | `63b866b2a7` | feat(strict): preserve scalar property conversion references |
| 81 | `94e873a4c5` | feat(strict): preserve nullable property reference contracts |
| 82 | `e2b20ae08a` | feat(strict): preserve object property conversion references |
| 83 | `15faacbb39` | feat(strict): preserve array property conversion references |
| 84 | `3ca4036dd5` | feat(strict): preserve iterable property conversion references |
| 85 | `82c96130e7` | feat(strict): preserve boxed static conversion references |
| 86 | `9e2878062e` | feat(strict): share checked static reference exports |
| 87 | `05382f8d22` | feat(strict): validate eval reference replacement before publication |
| 88 | `4ecc79b427` | feat(strict): preserve concrete static conversion references |
| 89 | `a958aab372` | feat(strict): validate static publication before initialization |
| 90 | `aceb5004d3` | feat(strict): contain scoped runtime static property writes |
| 91 | `8ddce6abf6` | feat(strict): preserve exact declaration reference contracts |

| 92 | `69bb593ad5` | feat(strict): share exact eval property value contracts |

| 93 | `da8624a166` | feat(strict): validate live eval property reference writes |

| 94 | `eae00db419` | feat(strict): guard native writes with live eval property sources |

| 95 | `c0130103c7` | feat(strict): check evaluated builtin input contracts |

| 96 | `3fb2cfffc5` | feat(strict): guard source builtin inputs and reference owners |

| 97 | `f36b2321b0` | feat(strict): validate builtin inputs before folding |

| 98 | `4d3192488e` | fix(strict): retire converted reflection class arguments |
| 99 | `b8f700b046` | feat(strict): guard native builtin input contracts |
| 100 | `87db018d1b` | feat(strict): implement exact scalar comparison boundaries |

| 101 | `bed10dc9b4` | fix(strict): preserve comparison sources and switch lifetimes |

| 102 | `c7b1e9b122` | fix(strict): retain numeric producer policies and result bindings |
| 103 | `40185b8f12` | feat(strict): guard builtin comparison modes and callable boundaries |
| 104 | `018184aad7` | feat(strict): enforce native string and formatting contracts |
| 105 | `9da37f1595` | feat(strict): enforce eval string and formatting contracts |
| 106 | `2d69515576` | fix(strict): validate native string updates before mutation |
| 107 | `73105c526a` | fix(strict): reject logical null string operands |
| 108 | `6904bee88a` | fix(runtime): preserve logical null string identity |
| 109 | `3bc3f47cf4` | feat(strict): enable string contracts across native hosts |
| 110 | `ba2bc9f312` | feat(strict): admit exact value contracts across native hosts |
| 111 | `4727b75ace` | feat(strict): enable checked casts across native hosts |
| 112 | `c30508e838` | fix(eval): validate comparator returns and retire sort owners |
| 113 | `7a97ec5e68` | feat(strict): validate native comparator result kinds |
| 114 | `82300f746f` | feat(strict): enable noncoercive comparisons across hosts |
| 115 | `6daf631e8b` | fix(eval): publish only live native object binding contracts |
| 116 | `1cb2b34778` | feat(strict): enforce stable numeric local bindings |
| 117 | `f1b64aa4cd` | feat(strict): guard nullable numeric binding updates |
| 118 | `bfab7777ec` | feat(strict): preserve numeric caller reference contracts |
| 119 | `4b72179d63` | feat(strict): preserve native numeric contracts during eval |
| 120 | `08d7c78270` | feat(strict): preserve scalar and nominal local value contracts |
| 121 | `bd5ed6f699` | feat(strict): preserve local contracts during mutating conversions |
| 122 | `d6d6557899` | feat(strict): preserve native bindings across literal eval |
| 123 | `95d4fbe5e9` | fix(eval): balance scope finalizer value owners |
| 124 | `86ff70f4cc` | fix(strict): validate literal eval writes in source order |
| 125 | `cb76d22fb7` | feat(strict): track fresh literal eval binding origins |
| 126 | `824dbed7b4` | feat(strict): preserve binding presence across eval snapshots |
| 127 | `1da05774fc` | fix(strict): preserve eval origins across native writes |
| 128 | `b5fec3b5c6` | feat(strict): retain runtime eval binding origins |
| 129 | `60a1d1c370` | feat(strict): preserve native eval producer contracts |
| 130 | `bd5dfddb55` | feat(strict): preserve conditional native binding origins |
| 131 | `ed4423fae2` | fix(strict): validate native writes against current eval origins |
| 132 | `bba9d412bd` | feat(strict): retain independent native copy contracts |
| 133 | `b3e9fd2292` | feat(strict): preserve callable and iterable binding contracts |
| 134 | `ac5e3f53aa` | feat(strict): preserve resource local binding contracts |
| 135 | `f3f1a66f47` | feat(strict): retain interpreted parameter binding origins |
| 136 | `1ce7e03b84` | feat(strict): preserve closure capture binding origins |
| 137 | `f59ccee500` | fix(eval): retire reflection snapshot temporaries |
| 138 | this commit | feat(strict): preserve interpreted static binding origins |


### Latest verified admission checkpoint

Verified178 admits ArrayKeys as the eleventh feature. Public PHP/LFC checks,
native/direct and opaque eval, exact key producers, original-source/optimizer
rules, Reflection diagnostics, cache identity, immutable FFI profiles and twenty-four
CLI/example cases are verified. All twenty-seven focused gates pass, including
real assembly for every supported target and the builtin documentation workflow.
The retained unserialize wire and its missing protected exception boundary are
repaired. Native execution is Linux x86_64; all supported targets have emission
and assembler coverage.

Five families remain pending: Nullability, Mixed, Initialization, ArrayTypes and
Indexing. Stronger Locals and complete umbrella acceptance remain active work.
No push, PR or full local suite has been requested or run.

### Completed Coercions and Casts admissions

Signed110 ba2bc9f3129563987301ab391d3578e9a8b314a6 admits Coercions.
Signed111 4727b75aced908c09988daed550f7d5e602c4b17 admits Casts.
The [reference and cast admission history](strict-semantic-profile-references.md)
preserves all intermediate failures, implementation safeguards and final proof.
Independent native/eval/proposed declaration candidates, stable static rebinding,
Reflection temporary retirement, capture/unset, COW and context lifetime checks
are complete. These former Casts admission issues are not open work.

### Next: source-point bindings and stronger Locals

The full authorized task continues with autonomous verified commits. Baseline
Step116 supersedes baseline knowledgea9725473: ordinary inferred integer
reassignment to float is rejected, and PHP overflow promotion is checked before
storing into an integer binding without enabling Overflow. Step115 resolves knowledgeffd0deed and the original knowledgea7883b31:
a future native catch class no longer constrains an earlier opaque eval using the
same name. The exact fixture tests/fixtures/strict/catch_scope_contract.php now
prints PHP's TVN in both optimizer modes with clean heap accounting.

Native eval snapshot contracts must follow binding/value provenance at the eval
site, not a future frame slot's inferred Object class. Reuse local-store contracts,
rooted membership guards, generic_local_contracts and persistent reference sources
for the stronger Locals work. Preserve explicit union/mixed contracts, array-kind
versus element-contract independence, parameter/capture/global/reference semantics,
loop joins and legitimate unset/rebind behavior. The completed baseline and
Comparisons temporary validation directories are removed after evidence capture.


### Current local binding mechanisms through signed133

Detailed115-133 investigation and acceptance evidence is retained in Git commit
`b3e9fd22920cccfbd51886045a5ff39cb1e74aef`, Code Journal plan revision205,
work log47d89b07 and architecture document3739892b. Those records preserve the
intermediate failures; the current implementation uses these shared mechanisms:

- Checker local store/copy contracts retain semantic producer alternatives across
  read narrowing, AST propagation and physical widening. Inferred arrays preserve
  PHP array kind independently of ArrayTypes; explicit mixed stays open. Conditional
  flow joins preserve binding presence and original contracts.
- Reference callback plans validate the rooted original candidate before conversion
  or publication. Existing scalar, nominal, structural and callable membership
  checks are shared. NormalizeCallable results own their descriptor/receiver leases.
- Literal and runtime eval copy successful binding origins into activation-owned
  scope metadata. A private provenance word distinguishes absent, native and eval
  origins. Native writes and independent copies select the surviving contract;
  unset ends only the detached name's origin. Immutable ABI1 metadata records live
  in elephc-semantics; weak allocation tokens do not own PHP values.
- Scope publication is ordered with source effects. Finalizers copy values before
  destructive stores and retire temporary owners. Lazy scope materialization covers
  skipped eval branches, suspended callbacks and first post-eval locals.
- Callable/iterable locals use actual callable lookup or array/Traversable membership.
  Late guard dependencies belong to their reachable body, including lexical closures.
  Native user-function lookup is case-insensitive on both architectures.

Latest signed133 acceptance, jobef8b00e4, passes the build/default archives and nine
exact gates: all-five-target EIR and real clang program/runtime assembly, live/dead
callable dependency controls, prelude reservation, walker/VM declared callable owner
checks, both-optimizer native capability/alias/expression/static/trait/invokable
cases (6.50s), example (37.05s), and opaque sort ownership (58.99s). Native default
217/217 and strict265/265 allocation/free counts are clean. Assembly comments,
diff hygiene and SSH commit signature are verified.

### Verified134: resource local binding capabilities

Fourteen neutral builtin producer contracts now describe resource or resource|false
results. Native check hooks retain false precisely, and native/eval binding origins
share resource tag membership. Resource subtypes remain interchangeable within this
PHP value-kind contract. Generated metadata uses a reserved @resource atom so a PHP
class named resource keeps its ordinary class identity. LocalBinding and Return
positions admit that atom; unsupported resource parameter staging remains rejected.
The new eval metadata variant is appended, preserving earlier serialized indices.

Canonical promotion and raw-to-Mixed boxing retain semantic Resource metadata
separately from integer ABI storage. EIR already retained the correct producer type;
using codegen_repr at the boxing boundary had changed rejected old values into
integers. Both ordinary and canonical property boxing now preserve their resource tag.

The full builtin-docs workflow passes in2f860c9c: 1312 exported builtins, 2484 rendered
pages, zero builtin/site audit errors and zero target architecture inventory errors.
Eight original exact gates passed. Correctede9140a8d passes all-five-target EIR and
the native bundle (157.60s, IR on/off, default/Locals, clean heaps), including aliases,
expressions, literal/opaque eval, independent copies and unset/rebind. Final6ac5f436
passes the default build (47.84s) and walker/VM resource alias owner gate (0.07s),
completing all eleven exact gates. Earlier fixture-only failures concerned VM admission,
assembly spelling and a conditional unset shape. Assembly comments and diff hygiene
pass; only inherited Rust warnings remain. No full suite, push or PR was run.

The next bounded baseline is explicit: interpreted parameter bodies currently lose
their declared binding origins, and inferred eval closures retain an incidental
signature instead of callable capability. Default W11|W|W|W| versus Locals
T2|W|W|W| confirms both gaps with clean57/57 and75/75 heaps. The next change transfers declared parameter origins through the shared binder and
records inferred closure capability directly, with mixed/untyped controls.

Continue all approved work through complete Locals flow, Nullability, Mixed,
Initialization, ArrayTypes, ArrayKeys, Indexing and final host/umbrella acceptance.
No full local suite, push or PR has been requested or performed.

### Verified135: interpreted parameter and callable origins

The shared interpreter parameter binder transfers declared binding kinds into scope
origins for functions, closures and instance/static methods. Union/null alternatives
survive, mixed/untyped remain open, and variadic bindings are arrays. Nominal self
and parent atoms resolve through the existing declaration-scope parameter helper.
Inferred closure/first-class callable producers retain callable capability rather
than their first incidental signature. Explicit local declarations retain their
existing structural checks. No callee-only type is permanently installed on caller
storage merely by entering the binder.

Prooff33c2615 passes default build, two new exact caller/owner gates, the existing
Coercions parameter control and native parameter/alias/method/closure/variadic/unset
cases in1m29s. Default289/289 and Locals480/480 allocation/free counts are clean.
Finala75f3ccf passes build and five exact gates in4m34s: both new owner regressions
(including false controls), the existing Resource alias/owner regression, permanent
both-IR/default/Locals native bundle176.48s with self/parent cases, and the focused
runtime-plugin example37.98s. All native heap reports are clean. CLI docs explain
parameter, variadic and inferred-callable rules. Hygiene passes and only inherited
warnings remain; no target-specific assembly or external ABI changed.

The next related baseline ec663cb8 measures by-value/reference captures, escaping
closures, mixed/union/nullable sources, fresh capture bindings, globals, static
locals and foreach. The result confirms missing by-value/escaping capture, static and foreach origins;
reference captures and globals already enforce theirs. Continue broader binding flow,
all six pending families and final host/umbrella acceptance. No full local suite,
push or PR has been requested or run.

### Verified136: immutable closure capture origins

EvalClosureCaptureBinding retains an optional immutable producer contract. Creation
copies it before capture evaluation; each activation installs it in the existing
scope origin map. The metadata owns no PHP cells and follows existing Clone paths
through escaping, nesting and bindTo. Mixed/union/null and nominal alternatives
remain intact, and unset permits a fresh local binding.

Job0a1516c5 passes default build and all four exact gates in3m28s: new walker/VM-
caller capture ownership alternatives, existing escaped property-reference owners,
permanent both-IR default/Locals native capture lifetime bundle121.70s and updated plugin
example37.09s. Coverage includes nullable/mixed/union, array-kind independence,
callable lookup, Resource|false, nested/escaping/bound closures, self after rebinding
and rejected-value preservation. All native heap reports are clean. Hygiene passes;
only inherited warnings remain. No target emitter or external ABI changed.

Static and foreach origin gaps from ec663cb8 remain open. The next static integration
must preserve initialization order, activation/capture lifetime, legitimate unset
and Reflection snapshot semantics. Reference PHP prints1|2|11| for ordinary static
snapshots, already-escaped reference snapshots, and unset/new-local replacements.
Reuse reference_is_shared before retaining a snapshot, plus the existing canonical
reference/origin coordinator. Foreach needs the array producer/element policy audit
alongside ArrayTypes. Continue all six pending families and final host/umbrella
acceptance; no full local suite, push or PR was requested or run.

### Verified137: Reflection snapshot temporary ownership

Static and captured-variable snapshots now use the shared EvalArrayBuilder, which
retires each owned key and retained value after the result array takes ownership.
The related baseline exposed thirteen leaked blocks (456 bytes) in both profiles.
The FakeOps object identity helper also resolves reference wrappers before returning
the underlying identity, matching native object identity and Reflection registration.

Default build passes in11.02s. Native snapshot regression passes119.51s in both IR
modes and default/Locals, with every heap clean. Final job1905ad17 passes the two
exact fake identity/snapshot owner regressions in10s; all fake cell owners reach
zero. The earlier UnsupportedConstruct was isolated to getClosureUsedVariables
under Locals, then traced to the mock wrapper identity. No target emitter or ABI
changed. Diff hygiene passes; inherited warnings remain. Static origins and the
six pending families continue next; no full suite, push or PR was run.

### Verified138: interpreted static binding lifetime

The shared static coordinator retains producer origins with persistent context
cells, lends owned activation leases, and tracks only reached, still-attached static
bindings. Initialization succeeds before detaching an older local. Recursive first
successful initialization wins, and the new binding is published before retiring a
displaced value whose destructor can throw. Unset/reference/global rebindings leave
the persistent value intact. Reflection shares already-escaped references and copies
ordinary unescaped snapshots through the existing array metadata and owner helpers.

Firstc8f9fab3 passes build10.86s, existing snapshot owners0.05s and both-IR/default-
Locals native lifetime bundle132.63s with clean heaps. An array-identity assertion
is corrected to compare copied contents. Final58d40c31 passes the default build10.39s
and five exact gates in4m03s: declared initializer alternatives/owners0.08s,
snapshot owners0.07s, recursive/detached/displaced-destructor owners0.08s,
extended both-IR/default-Locals native lifetime bundle143.40s and plugin example48.20s.
Every native heap is clean and all fake cell owners reach zero. PHP validates the
extended default output. Hygiene and PHP syntax checks pass; only inherited Rust
warnings remain. No target emitter or external ABI changed.

Native baseline confirms late declarations still reuse the wrong frame slot,
conditional declarations retain a prior local, unset mutates persistent storage and
throwing initializers lose the old value. Native dynamic and union static stores
already pass; mixed-reference static arguments are explicitly rejected in both
profiles. A bounded EIR draft separates persistent metadata from activation aliases
using existing raw payload layouts and reference binding helpers. No native draft
has been applied. Validate heterogeneous reference payloads and old-origin retirement
before accepting that integration; managed storage is a separate design option.

Reflection also needs dynamic-initializer placeholders: reference PHP reports null
without running a function/parameter initializer before its first call, then exposes
the initialized value. Current reflection evaluates missing initializers. Factb876b275
records this open gap; do not cache its placeholder as a successful binding.
Continue native/static and foreach flow, all six pending families, and full host/
umbrella acceptance. No full suite, push or PR was run.

### Verified139: native static activation lifetimes

Function static metadata now owns a canonical managed cell with a boxed physical
payload. The PHP name remains an activation alias, retained after successful
initialization. Prior aliases keep their original binding. Displaced values stay
pinned until publication, and process/request/application cleanup detaches the
persistent owner before releasing it. Top-level statics retain their existing path.
Original initializer producer contracts survive narrowing, including open mixed.
Canonical writes reuse the existing Mixed publisher independently of validation.
Scalar string reference arguments preserve the original place instead of casting
to a retired value temporary. AST/EIR effects include static publication callbacks.

Final queue b631f49e passes in2m50s: default build38.66s, all-five-target EIR0.75s,
real clang1.51s, settype-foreach1.01s, optimizer destructor boundary0.01s, expanded
native bundle7.93s and existing valid-reference/union bundle1.15s. Twenty-two probe
outcomes are preserved; nineteen execute with clean heaps, while three inherited
checker refusals remain for the next step. The new static example passes both IR
modes with clean heaps. The previous interpreted lifetime bundle also passes in
queue4ba0b630 (135.75s). PHP output, syntax and hygiene checks pass.

The first raw-address prototype failed default float writes and exceptional reads;
canonical cells correct both. A final effect test initially lacked any possible
destructor, so closed-world catch removal was valid. Its corrected real-destructor
fixture passes without changes to exception-flow precision.

Next: detach a reached static activation on unset, accept boxed reference writes
through its existing canonical cell, and remove checker restrictions that assume
raw static payloads. Dynamic Reflection placeholders, foreach producer flow, all
six pending semantic families and final host/umbrella acceptance remain active.
No full local suite, push or PR was run.

### Verified140: native static detachment and reference writes

Reached nonmain statics install the existing canonical boxed-reference proof and
binding depth after successful inference. Rebinding retires old parameter/global/
reference roles. Unset consults its current proof before invalidation, then the
existing detach and kill mechanisms end only that activation name and origin.
The persistent cell and its immutable validator remain alive. Mixed reference
formals and escaped reference returns share that same cell; string updates use
ordinary checked semantics when canonical storage is proven. No new runtime ABI
or target emitter is needed.

Final queue316f0948 passes in2m51s: default build36.97s; all-five-target storage/
owner gate0.72s; fresh-binding diagnostic0.04s; both-IR/default-Locals native
reference/detachment bundle2.39s; prior activation/lifetime bundle7.61s. All22
native probe programs now compile and execute with clean heaps, resolving the
three inherited checker refusals from139. The expanded example passes both IR
modes with clean heaps. PHP reference output/syntax and diff hygiene pass.

These straight-line source-point mechanisms preserve conservative conditional/
eval detachment gates. Continue remaining control-flow/global/foreach audits,
dynamic Reflection placeholders, all six pending families and final host/umbrella
acceptance. No full local suite, push or PR was run.

### Verified141: shared pure scalar folding

Scalar facts, PHP numeric-string comparison and finite numeric folding now live
in `elephc-semantics`. Compiler adapters retain AST reconstruction, source literal
eligibility and explicit profile selection. Existing algorithms preserve bit-exact
propagation identity, integer-exact/string ordering, PHP promotion, exact division
and declined warning/error cases. Reflection itself is unchanged in this step.

Default archives build in2m11s and three neutral tests pass in ef9f0c60. After the
session became unavailable, only the remaining gates resumed in2607b1c5 (3m50s):
nine exact compiler edge/profile matrices and two native controls pass, including
integer powers/division, comparisons, numeric strings, NaN, casts, signed zero,
strict source evidence and independent numeric domains. No unrelated suite ran.

Continue safe Reflection initializer reduction using these shared primitives.
Dynamic placeholders, remaining binding-flow/foreach audits, all six pending
semantic families and final host/umbrella acceptance remain active.

### Verified142: silent Reflection constant preparation

Reflection prepares reached constants without executing dynamic initializer syntax.
Shared static publication preserves the original producer contract; deferred null
placeholders install neither storage nor origin. Preparation roots and retires all
intermediate owners. Neutral arithmetic retains silent IEEE metadata while the
compiler keeps its finite folding policy. Structural array union/equality reuse
key and scalar primitives. Context teardown now also drains define-created constants.

Final9c264422 passes in8m32s, including updated archive build9.90s and native
before/after Reflection154.30s, prior static origins155.26s and snapshot148.01s.
Those bundles cover both IR modes, default/Locals profiles and clean heaps. Together
with unchanged numeric/compiler gates in d5660a60, all15 unique exact tests pass.
Walker/VM callers, pending Throwable preservation, absent deferred storage/origins,
array key/order/recursive equality, PHP8.5 reference output and diff hygiene pass.
No runtime ABI, target emitter or builtin contract changed.

Continue foreach value-before-key publication and callback-safe owner retirement,
then structural producers, remaining binding-flow audits, all six pending families
and final host/umbrella acceptance. General dynamic boxed eval array operators still
need their proper structural boundary, recorded separately in factd9f697ef. No full
suite, push or PR was run.

### Verified143: foreach publication and retirement

Array, Iterator and IteratorAggregate traversal share a focused owner helper.
Current and optional key are acquired before either binding changes, then value
is published before key. All displaced/pending leases retire even on callback
failure. The consuming scope setter balances identical aliases. Source cleanup
keeps pending return/throw values rooted, and no-key traversal skips key work.

Exact605c12aa passes walker/VM callers with default/Locals and zero remaining
owners. Final88f16b7f passes in4m08s: archive build10.50s, three focused receiver/
Traversable unit regressions, both-IR/default-Locals native publication/GC/throwing
cleanup155.19s and existing eval aggregate owner bundle49.57s. All six exact gates,
PHP8.5 fixture output/syntax and diff hygiene pass; native heaps are clean. No ABI,
target emitter or builtin contract changed.

Structural foreach producer alternatives remain open alongside ArrayTypes.
Continue source-point binding-flow/global audits, six pending semantic families
and final host/umbrella acceptance. No full suite, push or PR was run.

### Verified144: exact runtime key identity

The immutable ArrayKeys bit now selects identity at `__rt_hash_normalize_key`:
strings keep their original byte pointer and length on every supported target.
PHP mode keeps its canonical integer-string conversion. Hashing and equality
retain integer/string kind and original values independently of hash identity.

Final60ac5427 passes in1m40s: archives68.07s, real native helper probe0.67s and
all-five-target/two-profile clang assembly0.68s. Fifteen canonical/binary key
spellings per profile, embedded NUL, integer endpoints and an engineered full
integer/string hash collision preserve the expected key identity. Assembly
comment alignment and diff hygiene pass; the probe scratch directory is removed.

This is a runtime component, not public ArrayKeys admission. Continue exact key
shape inference, pre-fold validation, EIR/eval key guards, constructors/builtins
and cache/combination coverage before enabling it. Foreach producer alternatives,
remaining local-flow audits, all six pending families and final umbrella acceptance
stay active. No full suite, push or PR was run.

### Verified145: profile-aware array key shapes

The immutable profile now flows explicitly through syntactic expression/return
inference, declaration defaults, loop storage, lvalue checks and EIR fallback
classification. Exact strings retain their kind, including value-derived builtin
keys. Parser append storage conservatively supports either key policy without a
parser-global semantic switch.

Finald67b1a33 passes in4m07s: default archives73.41s, updated all-target selected
frontend2.59s, both-IR/default-exact native shapes1.79s, four existing inference/
PHP-normalization/append controls and builtin exporter61.63s. Together with the
two unchanged metadata/recursive-inference units in410c0442, all eight unique
exact gates pass. PHP/Internal sources, nested numeric-string appends, clean heaps,
assembly comments, docs regeneration, three audits and diff hygiene pass. Generated
pages and registries stay unchanged.

Public ArrayKeys remains gated. Continue optimizer key identity, original-source
and dynamic domain checks, spread/default/reflection identity, eval/reference
metadata and constructor parity. Foreach producer alternatives, local-flow audits,
all six pending families and final umbrella acceptance remain active. No full
suite, push or PR was run.

### Verified146: optimizer array key identity

Literal array reads use the existing scoped profile and neutral key classifier.
Exact strings keep their kind; duplicate keys replace only the matching typed
key. Invalid bool/float/null keys decline folding even in unselected entries,
retaining the original domain failure for later validation and runtime checks.

Job825f14bb passes in2m03s: archives70.04s, two key-fold units, all-five-target
selected checker/AST/EIR optimization1.02s and native optimizer parity1.30s with
clean heaps. Jobd4e4fce2 passes the three existing PHP normalization/lossy-float/
indexed-key controls. All seven unique exact gates, PHP fixture equivalence and
diff hygiene pass. Numeric strings, actual duplicates, propagated reads, nested
arrays, embedded NUL and independent string offsets are covered.

Public ArrayKeys remains gated. Continue original-source and dynamic key-domain
checks, spread/default/reflection identity, eval/reference metadata and constructor
parity, then the remaining binding audits, six pending families and final umbrella
acceptance. No full suite, push or PR was run.

### Verified147: original array key domain validation

Shared original-key membership distinguishes accepted kinds, rejected alternatives
and dynamic guards without enabling another strict family. Source checks and
literal eval share one guarantee inventory. Typed array places validate before
normalization, including references, nested/property/static writes, probes and
autovivifying parents. String offsets, ArrayAccess and unknown receivers keep their
own rules; dynamic keys remain admitted for the following runtime guard work.

Preflight05d4730e passes all three new exact units. Final95ef3782 passes in1m42s:
expanded all-target place/probe checks0.80s, archives61.33s, existing all-target
shapes2.26s, native shapes1.67s, native full-optimizer parity1.32s and original
literal/dynamic-eval source control0.28s. All seven unique exact gates and diff
hygiene pass. Native heaps stay clean. No ABI or target emitter changed.

Public ArrayKeys remains gated. Continue receiver-aware dynamic key guards and
owner/effect handling, then spread/default/reflection identity, eval/reference
metadata and constructor parity. Remaining binding audits, all six pending
families and final umbrella acceptance remain active. No full suite, push or PR.

### Verified148: receiver-aware key reads and construction

Rooted EIR membership checks original int|string keys only on PHP-array receiver
paths. Ordinary reads, native isset and associative literal construction preserve
string/object dispatch and original operands. Published receiver/value owners
survive catchable key failure. AST effects and exact TypeError flow retain guards
and catch blocks when the literal result is otherwise unused.

Retry2c8fbab7 passes selected all-target and native checks. Final7310641f passes
in1m10s: archives62.65s, existing all-target structural membership1.93s and parameter
checks1.41s, native shapes1.85s and optimizer parity1.51s. Global96607b0c passes
PHP/Internal all-target generation32.93s and eight native mode/profile/optimizer
variants4.09s with clean heaps. All six unique exact gates and diff hygiene pass.
Temporary payload destruction precedes catch entry and remains safe during GC.

Initial87ab6060 exposed a preexisting boxed ArrayAccess raw-key mismatch, not an
owner failure. The generic getter materializes a hash tuple before receiver
dispatch, so boolean true arrives as integer1. PHP preserves boolean. Factcc2980d1
records the clean-heap reproduction. The current control tests object acceptance;
raw argument-kind parity is explicitly pending, never claimed from that control.

Public ArrayKeys stays gated. Next repair raw boxed-key dispatch, then mixed/spread
construction owners, fetch-for-write/mutation paths, builtins, default/reflection/
spread identity and eval/reference metadata. Complete remaining binding audits,
all six pending families and final umbrella acceptance. No full suite, push or PR.

### Verified149: original boxed offset dispatch

Shared ABI helpers now materialize boxed keys for both EIR and runtime code.
Ordinary generic reads carry original boxed operands or raw string tuples until
receiver dispatch. PHP ArrayAccess getters receive detached value snapshots;
arrays and native SPL containers retain their own key conversion paths. Existing
fetch-for-write callers keep their tuple protocol pending their separate audit.

Native regressions cover original bool, float, null, integer, numeric string,
array, object and resource keys, computed numeric keys, COW and throwing getters.
Both profiles and optimizer settings pass with clean heaps. Twenty user/runtime
assemblies pass clang across all five targets. Float warning and native SPL
controls pass. A cache warmed with signed148 creates a different runtime object
with149, changes the PHP getter argument from integer to boolean, and reuses the
new object on the next compile. Default compiler/bridge builds, assembly comments
and diff hygiene pass. Public ArrayKeys admission remains gated.

### Verified150: mixed literal construction owners and key checks

Mixed keyed/spread literals reuse published container slots, rooted operands and
original-key guards. Hash growth writes back into the construction slot. The RHS
is evaluated and rooted before key validation; partial arrays and temporary operands
retire on key, spread and later-expression exceptions. Spread inputs retain their
own owners and pointer-valued storage remains unchanged.

Job39ab46c8 passes all-five-target generation and eight native mode/profile/optimizer
variants (2.03s/2.91s), with clean heaps. PHP cross-check confirms default spread
renumbering and Error classification. Finalb3d8cb95 passes in1m42s: default compiler
and bridge builds49.52s plus five exact legacy spread-owner and pointer-storage
controls. All seven unique gates and diff hygiene pass. ArrayKeys remains gated.

### Verified151: profile-aware constant and default keys

Constant default keys share the neutral byte classifier with native hash semantics.
Spread-free mixed planning receives an explicit profile, so a numeric string no
longer advances the exact integer append cursor. Literal storage copies, Reflection
defaults/constants/source exports and eval user-constant metadata retain the same
profile. Reflection and direct storage reuse the shared literal-key adapter.
Proven bool/float/null exact keys decline metadata folding without PHP conversion.

Job696d6fd6 passes metadata, twenty mode/profile/target generations42.12s and eight
native mode/profile/optimizer variants27.39s with clean heaps. Direct calls,
first-class callables, Reflection invocation/default lookup, instance/static
properties and global/class constants preserve distinct numeric strings and append
history. PHP output matches the permissive control. Final269bebca passes in3m26s:
default compiler/bridge builds77.44s, shared default/enum/literal-planner ownership
controls and four exact legacy Reflection/default integration tests. Assembly
comments and diff hygiene pass. ArrayKeys remains gated pending its other paths.

### Verified152: profiled call planning and unpack ownership

Static spread/call planners, CallArgPlan projections and generic inference carry
explicit key policy. Checker/EIR, numeric source proofs, Reflection argument
adapters, literal-eval folding/safety and typed reachability use the same profile.
Parser syntax validation is separate from profile-dependent static unpack order.
Raw-source validation no longer mistakes values expanded from a second unpack
for written positional arguments after an unpack.

Reflection construction reuses the dynamic constructor helper with original
arguments, avoiding a second variadic collection. A raw iterable returned as Mixed
has a separate boxed payload retain, so caller cleanup now releases its source
owner; direct iterable returns keep their pass-through alias behavior.

Jobae5b4cd3 passes conversion independence12.07s, twenty mode/profile/target
emissions37.40s and eight native variants25.55s, all heap clean. Cases cover numeric
string names, duplicate integer/string identity, runtime sources, callbacks,
constructors, Reflection, separate-spread append order and NUL bytes. Regular float
constructor binding and independent Coercions checks pass. Reference elements in
AOT array literals remain an explicitly diagnosed unsupported source construct.
Finald952d76c passes in3m03s: compiler/bridge builds68.91s, isolated owner probes,
new eval/generic/origin gates, default provenance controls and six exact legacy
named/spread/raw-iterable regressions. All20 unique gates, assembly comments and
diff hygiene pass. Pre-check dependency policies and duplicate named-key RHS
effects remain to audit; ArrayKeys and the six pending families stay gated.

### Verified153: evaluated duplicate-key unpack sources

Static bindings preserve first key position and last value for inference while
retaining the original array for execution whenever overwritten RHS expressions or
float-key diagnostics could disappear. Positional projections preserve producer
metadata; float identity uses PHP conversion. Opaque fixed-signature hash unpacks
reuse boxed argument binding. Reflection newInstanceArgs evaluates its actual
array before constructor dispatch. Shared focused lowering setup replaces duplicate
test infrastructure.

Jobf7fb76bc passes two planner/inference gates, warning-order native checks, ten
profile/target emissions25.61s and eight native mode/profile/optimizer variants
32.98s with clean heaps. The fixture covers overwritten effects and throws, object
destructors with GC and final iteration order. PHP output agrees. Final83615103
passes in5m09s, including compiler/bridge builds123.29s and fourteen exact generic,
named/spread and Reflection constructor controls. All19 unique gates, assembly
comments and diff hygiene pass. Function/Method invokeArgs still require actual
array materialization; ArrayKeys and the six pending strict families stay gated.

### Verified154: evaluated Reflection invocation arrays

ReflectionFunction and ReflectionMethod invokeArgs share actual source-order
argument materialization through the call planner. Original array kinds are
validated without converting Traversable objects; rooted parameters outlive
callee dispatch, and independent boxed results survive source and receiver
cleanup. Removed obsolete argument-expression snapshots from local assignment,
speculation and control-flow joins. Added examples/reflection-invocation.

Job0dac6d4d passes ten target/profile emissions23.16s and eight native
mode/profile/optimizer variants30.23s with clean heaps. Duplicate RHS effects,
runtime arrays, named source order, returned objects and caller/callee throws
match PHP. Final70c5c723 passes in4m58s: compiler/bridge builds90.46s and nine exact
legacy Reflection, constructor, Coercions and Casts controls (including opaque
eval65.29s). All11 unique gates, assembly comments and diff hygiene pass.
Example3ee32293 compiles/runs with php-only and Coercions in4.02s.

The original full Reflection example succeeds without flags but fails under
php-only plus Coercions with a missing ReflectionParameter default literal data
ID, independently of invocation arrays. Investigate that strict metadata path
next. ArrayKeys and the six pending strict families remain gated.

### Verified155: constructor value guards and caller references

Dynamic constructor selection distinguishes actual by-reference parameters from
owned cells backing ordinary typed properties. User value contracts independently
request fixed binding through conversion_boundaries::active before ABI conversion;
trusted Internal default machinery and hidden collectors retain their own roles.
This removes speculative Reflection subclass construction from ordinary default
metadata while preserving PHP/LFC scalar checks and promoted caller references.

Job3acd2d42 passes ten profile/target generations58.21s and sixteen native
metadata/value-contract variants90.47s with clean heaps. PHP, LFC and Internal
policies, optimizer settings and owning user arguments are covered. Final0550cfd3
passes in2m33s: compiler/bridge builds36.32s, five exact promoted-reference target
controls and five integration controls. A bounded captured-argument fixture helper
supplies the unpruned catalog's conversion declarations. All12 unique gates,
assembly comments and diff hygiene pass. The original full Reflection example
passes with php-only plus Coercions4.50s; tagless LFC with heap checks passes9.36s.

Raw permissive dynamic constructor Mixed-to-string ABI conversion still leaves a
string owner; this is isolated independently and is the next cleanup step. The
optional Reflection subclass limitation reproduces without semantic flags and is
tracked separately as taskbc259636. ArrayKeys and six pending families stay gated.

### Verified156: constructor ABI string conversion owners

Constructor argument materialization adds string conversions to the shared cleanup
plan in source order. Borrowed formatting becomes persistent before later PHP
callbacks; already-owned results keep their owner. Slots use the target string
pointer register, normal retirement reuses canonical string release, and existing
owner records cover exceptions. Constructor entry points select this policy while
ordinary call-result alias rules retain their existing behavior.

Finald1b2adef passes in3m46s: compiler/bridge builds60.29s, ten target/profile
generations13.96s, six PHP/LFC/Internal native variants12.78s, five exact promoted
reference target controls and six string alias/throw/Reflection/eval controls.
All13 unique gates, assembly comments and hygiene pass. Ten clang assemblies cover
all five targets with both optimizer settings. Public CLIfe0da192 passes both
optimizer variants in6s with clean heaps. The fixture includes owned and borrowed
conversions, later formatting callbacks, conversion/callee throws, typed/untyped
reference writeback and four-string arguments overflowing native registers.

The signed154 raw constructor string-owner diagnostic is closed. Constructor-array
validation, dependency policies and the remaining ArrayKeys/Locals/six-family
integrations remain open; public admission has not changed.

### Verified157: shared reflected constructor array checks

ReflectionClass and ReflectionObject newInstanceArgs reuse the evaluated-array
planner, original array-kind validation, rooted parameters and independent result
staging already shared by Function/Method invokeArgs. Traversable arguments raise
TypeError instead of being converted to arrays. Constructor dispatch borrows the
rooted reflector, so argument-side receiver rebinding cannot consume or replace
its cleanup owner. Class-name operands retire with the other rooted arguments.
The invocation example now also constructs a delivery from a runtime array.

Joba443a7ec passes ten target/profile emissions49.44s and eight native
mode/profile/optimizer variants55.94s with clean heaps. Runtime arrays, named and
default arguments, inline/local reflectors, ReflectionObject, receiver rebinding,
source/constructor throws and returned object owners match PHP. Finala398994c
passes in3m27s, including compiler/bridge builds63.13s and six exact Reflection,
unpack and independent Coercions controls. All8 unique gates, assembly comments
and diff hygiene pass. Public CLIe034fd2f passes PHP with both optimizer settings,
LFC with Coercions, and the updated example in27.11s, all with clean heaps.

A separate receiver-rebinding constructor-dispatch failure reproduces on signed156
without semantic flags and is tracked as task4aa3b0e9. It is not claimed fixed by
array validation. ArrayKeys writes/producers/eval, dependency policies, remaining
Locals flow and the six pending guarantees still require completion. Public
admission is unchanged.

### Verified158: rooted original keys for array references

Array element references now share ArrayReferenceOperands for key evaluation,
owner publication, original-kind validation and ordered retirement. Ordinary
references, scoped reference arguments, addressable foreach aliases and the older
fetch-for-write path use the same preparation. Guards borrow published operands,
and keys stay alive until their reference binding is published. Cohesive array
reference lowering moved out of property_access into array_element_references.
This also retires fresh Mixed and string keys that the managed foreach path
previously leaked. ArrayAccess and WeakMap keep their separate receiver policies.

Job81d198d6 passes ten target/profile emissions1.84s and twelve native
PHP/LFC/Internal, profile and optimizer variants5.85s with clean heaps. Cases cover
bool/float/null rejection, exact numeric/padded string keys, ordinary references,
nested copy-on-write and throwing key expressions. Final4019627d passes in2m14s,
including compiler/bridge builds60.40s and seven exact read-domain, Mixed ownership,
foreach lifetime/COW and named/callable reference controls. All9 unique gates,
assembly comments and hygiene pass. Public CLI229d5fb5 passes both PHP optimizer
settings and LFC with Coercions in1.51s, preserving independent key policy.

A preexisting alias-reuse/null-sentinel crash from an outer foreach with unset is
recorded as facte22ed52c and remains pending in binding-flow work. The key tests use
separate bindings and do not claim that failure fixed. ArrayKeys writes, unset,
producers/eval, dependency policies, remaining Locals flow and all six pending
guarantees still require completion. Public admission is unchanged.

### Verified159: original keys at local write publication

Concrete and boxed local writes check original key kinds after required RHS effects
and before key conversion or publication. Prepared guards borrow rooted operands;
additional guard windows protect RHS and provisional receiver owners without
publishing an earlier lease twice. Pending-owner records retain their SSA identity
for this deduplication. Existing temporary cleanup and normal-path write ownership
remain authoritative. Null parents participate in the array-key check before
materialization; string offset policy remains separate.

Jobcf48be4d passes ten target/profile emissions1.50s and twelve native
PHP/LFC/Internal, profile and optimizer variants5.05s with clean heaps. Hash and
packed writes preserve snapshots, late variable keys, RHS effects and destruction;
boxed arrays/null parents and string offset controls preserve receiver dispatch.
Final1fa07799 passes in2m43s, including compiler/bridge builds73.28s and eight exact
key/source ownership, reference and throwing string-warning controls. All10 unique
gates, assembly comments and hygiene pass. Public CLI4ce8c36b passes both PHP
optimizer settings and LFC with Coercions in1.34s, all with clean heaps.

The full baseline probe separately exposes generic boolean-parent stores and
ArrayAccess key normalization gaps, recorded as fact3b17799e. Those runtime paths,
logical null keys from typed missing reads, remaining property/static writers,
unset, producers/eval, dependency profiles and outstanding binding/family work
remain pending. Public admission is unchanged.

### Verified160: original ArrayAccess write arguments

Erased ArrayAccess stores use the existing signature-aware EIR interface method
planner. Independent snapshots preserve original boolean, float, null, numeric
string, embedded-NUL, array, object and closure keys; native SPL and WeakMap
methods retain their own contracts. The shared borrow helper leaves ordinary
scalars Plain. Receiver detachment and call snapshots classify their leases
independently of the borrowed RHS of a null-coalescing assignment.

Signed commit `5c6e9f4e83` passes nine unique focused gates, compiler/bridge builds,
five-target emission and twelve native source/profile/optimizer combinations.
Public CLI PHP with both optimizer settings and LFC with Coercions match PHP with
clean heaps. The new null-coalescing object result regression closes the detached
receiver-clone leak without changing the assignment result's ownership.

### Verified161: boolean parents at boxed offset writes

Local boxed setters share the scalar-parent guard and reuse the existing nested
false-conversion helpers. A false parent becomes an empty array before its
warning. Returning handlers can replace the PHP variable; throwing handlers
complete int/string/bool dimensions, stop float/null dimensions, and chain their
exception beneath an invalid-key TypeError. Only actual array parents reject
array/object keys; ArrayAccess receives its original arguments.

Signed commit `f5fcf3ea5a` passes eight unique focused gates, compiler/bridge builds,
five-target emission, twelve native combinations and three public CLI variants,
all with clean heaps. The strengthened oracle distinguishes empty-array
publication from the later element insertion. Public admission remains ten
integrated families and six pending.

### Verified162: property, static and nested write boundaries

Property and nested stores use the shared boxed writer.
`boxed_write_owners` separates borrowed operand leases, representation-only key
boxing and caller retirement from the parent dispatcher. Concrete writers check
original keys before conversion. Mixed property targets defer their key contract
until the runtime array or ArrayAccess receiver is selected. Static writers retire
owned boxed keys and values while preserving borrowed assignment results.

A managed static store can consume its submitted clone and publish a different
cell. COW mutation reloads that published value rather than borrowing the retired
input; combined Coercions coverage closes the resulting stale-cell errors and
dropped static stores.

Ten focused gates pass, including compiler/bridge builds56.70s, twenty target/profile
emissions21.46s and twenty-four native variants86.29s with clean heaps. Exact
numeric-string keys, COW snapshots, source effects, owned key/RHS cleanup, setter
exceptions and assignment results pass across the supported targets and both
optimization settings, with ArrayKeys and Coercions independently enabled.
Public CLI PHP with both optimizer settings and LFC with Coercions also pass with
clean heaps. Throwable trace argument retention is a separate baseline
feature tracked as taskf82cb27c; this step does not claim default PHP trace-owner
lifetime parity. Unset, typed-miss null provenance, producers/eval, dependency
profiles and the remaining binding/family integrations remain open.

### Verified163: original keys and current storage at removal boundaries

Local, property, static and detached nested removals check the original key only
when the selected receiver is a PHP array. ArrayAccess receives its original
kinds; null/false removal behavior and readonly error priority remain intact.
`PreparedOffsetKey` shares representation-only boxing and its independent owner
ledger with writes. A shared local dispatcher evaluates the key first, then
selects the current packed, hash or boxed storage. Packed conversion derives its
element type from that current storage, including callback-driven replacements.

Twenty-four native profile/source/optimizer variants and twenty target/profile
emissions pass with clean heaps. All eight focused gates pass, including compiler/bridge builds in57.23s and
property-write/boolean-parent compatibility controls. Three public CLI variants
(PHP with both optimization settings and LFC with Coercions) pass in2.52s with
clean heaps. ArrayKeys remains pending
until typed-miss null provenance, producers/eval and other outstanding boundaries
are complete; no public feature admission changes in this step.

### Verified164: logical null from typed key producers

The common typed-array read preserves scalar misses as boxed null when ArrayKeys
is selected. It reuses the existing null-preserving getter path before scalar ABI
conversion, so direct dimensions, local copies, assignment expressions and prepared
producer policies share one origin rule. No per-consumer key evaluator or parallel
origin ledger is needed. Tagged integer reads retain their existing separate tag;
a legitimate integer 9223372036854775806 remains a valid key.

The regression covers typed int/string misses at read, literal, write, reference,
property, static and unset boundaries, explicit casts, valid empty-string/integer
hits, original null arguments to ArrayAccess and independent owner cleanup.
Twelve native source/profile/optimizer combinations and all five supported target
emissions pass. All six focused gates pass, including compiler/bridge builds in58.51s,
property-write and removal compatibility controls and existing nullable-read
regressions. Three public CLI PHP/LFC variants also pass with clean heaps. ArrayKeys and the full umbrella remain pending while the
remaining builtin, eval, dependency and binding/family work is completed.

### Verified165: exact eval key metadata and automatic indexes

Magician's array-key adapters receive the immutable semantic profile explicitly
and delegate string classification to the shared neutral key policy. The duplicate
integer-string parser is removed. Reference metadata, duplicate method-ref target
comparison, nested/local/property mutations, generic key validation and automatic
literal indexes now agree about exact string versus integer keys. Fake runtime
primitives declare the same fixed profile as their context.

Walker and VM regressions cover automatic indexes after a numeric string,
independent aliases, by-reference method targets, nested writes, property unset,
binary bytes and complete owner retirement under both exact and PHP-normalized
policies. The permissive fixture matches the PHP oracle. Separate duplicate-ref
literal alias behavior is recorded as fact815b580a and task792cad39; owned keys in
escaping generic reference metadata still need their own ownership treatment.
Compiler/bridge builds7.78s and all six exact compatibility gates pass in job
2ad37022 (19s total), including existing alias, append-history and static-reflection
owner regressions. No public feature admission changes in this step.

### Verified166: eval array key membership boundaries

A shared borrowed-handle guard checks original int|string membership only after
an eval operation selects a PHP array. Reads, explicit literal keys, local/property
stores and removals reuse it before key coercion or alias writeback. ArrayAccess
keeps its original keys. Staged walker/VM tests cover bool, float, null, array and
object rejection, valid scalar controls and complete exceptional owner cleanup.

The native fixture helper now uses production runtime requirements and link
planning for eval. Link preparation shares bridge resolution, SDK selection,
archive handling and cleanup; captured execution returns diagnostics to the
fixture instead of exiting its owning test process. Ordinary reference-only
fixtures retain their existing linker path.

Five-target staged emission passes in78.83s. Real native permissive eval passes
both optimization modes in58.46s with clean heaps. Public interpreter and FFI
entry points intentionally reject pending ArrayKeys: the separate strict native
gate is retained as ignored and MUST be enabled and passed before ArrayKeys
admission. This step does not claim strict native runtime admission. All nine focused gates pass, including compiler/bridge builds48.50s,
metadata/append controls and Linux, macOS and iOS linker regressions. Assembly
comments and diff hygiene pass.

Builtin/probe consumers, key producers, false/null parent parity and generic
escaped-reference key owners remain pending, followed by dependency profiles,
binding-flow alternatives and the other five unintegrated families.

### Verified167: original builtin key contracts and result ownership

Builtin home descriptors now declare reusable original-key inputs. The checker,
pre-fold source evidence, effect resolution and EIR lowering consume that contract
for array_key_exists, array_column, array_fill_keys and array_combine. Known invalid
inputs are rejected before dead branches disappear, across PHP, LFC and Internal
sources. Runtime value guards follow argument evaluation and preserve original
kinds through named/spread calls and prepared builtin callable aliases. Probe
constructs in eval share the same borrowed-key guard.

Eval key-producing adapters reuse EvalArrayBuilder for key-first insertion,
partial-result cleanup and balanced key/value leases. Fill/combine validate original
kinds before their legacy conversions; column extraction preserves selected owners
and nullable whole-row selection. Flip/count keep their existing skip policy for
invalid values while retiring all iterator, lookup and arithmetic temporaries.
Generated internals and registry JSON expose the shared input contract.

The regressions also corrected two concrete paths: typed array_column now uses the
shared profile-selected key materializer, and strict native isset borrows its rooted
receiver instead of releasing that lease twice. The latter previously made later
empty/coalesce reads miss even with an aggregate clean heap (fact92d9b006).

All 14 focused gates pass: four AOT native policy/optimizer cases in2.22s, two real
permissive native eval cases in55.92s, five-target emission in1.14s, original-source
proof and both staged eval engine/owner gates. Default compiler/bridge builds pass
in117.43s, exporter build in62.85s, and all required builtin generation/audit steps
pass. Diff and assembly-comment hygiene pass. Public ArrayKeys admission remains
closed; remaining runtime producer shapes, reflection, false/null parent parity,
escaped-reference key owners and dependency validation still require completion.

### Verified168: Reflection key forwarding and explicit capability boundaries

Known AOT ReflectionFunction/ReflectionMethod invoke and invokeArgs targets retain
original builtin keys through positional and named argument arrays. A shared
adapter records a rollback-safe source lowering refusal when an unresolved target
has a concrete Reflection receiver. Boxed/unknown receivers check their actual
class and raise an explicit Error only for the unsupported Reflection invocation;
ordinary user invoke/invokeArgs methods retain their own dispatch and values.

The implementation preserves existing permissive behavior and does not restore
Reflection targets from unchanged local slot epochs. PHP permits reconstructing
the same Reflection object through an alias (factbb54adb0), so that attempted
optimization was discarded. Legacy runtime Reflection fallback remains a separate
baseline task e7e7419e. The capability boundary is documented in the codegen guide;
ArrayKeys is still pending public admission.

All seven focused gates pass: four native PHP/exact optimizer cases in12.74s,
two native boxed receiver/exception owner cases in6.56s, five-target supported
emission in9.51s, five-target concrete refusals and boxed guard emission in19.35s,
both staged eval engines and policies with complete owner retirement, and the
existing conditional Reflection target control. Default compiler build passes
in123.58s without new warnings; PHP matches the permissive supported fixture.
Diff hygiene passes. Remaining source evidence, producer, parent-materialization,
escaped-reference, dependency and binding-flow audits continue before admission.

### Verified169: empty array parents, diagnostics and reference storage

Native boxed writes materialize a null parent before checking the original key.
Reference-bound indexed writers borrow their canonical child, matching the existing
append protocol, rather than detaching the in-flight write from its caller.
Normal value snapshots retain their PHP copy-on-write behavior.

Eval local/global/reference writes publish the empty array before false-conversion
diagnostics or invalid-key errors. The shared contextual error dispatcher invokes
handlers against that published state. A captured independent value and the owning
scope generation preserve handler rebinds and in-place writes without overwriting
them when the suspended write completes. Array/object key rejection remains active
in permissive eval, before a primitive can erase the original kind. The fake runtime
now promotes sparse integer writes instead of introducing nonexistent null cells.

The shared fixture covers 26 parent/key/reference/callback combinations plus a
copy-on-write control, PHP and exact policies, both optimizers, staged walker/VM,
five-target emission, native heap accounting and the real opaque-eval bridge.
Public ArrayKeys admission remains closed. Property/static publication, pending
handler exceptions, remaining producers, source evidence, escaped reference owners
and dependency profiles continue before feature admission.

All six focused gates pass: native PHP/exact policy and optimizer combinations
in3.09s, all five target emitters in1.44s, staged walker/VM and owner retirement
in0.29s, sparse FakeOps adapter regression in0.16s, real opaque eval with both
optimizers and clean heap in66.50s, and default compiler/bridge builds in75.10s.
No new compiler warnings; diff and assembly-comment hygiene pass.

### Verified170: ordinary property and static empty-parent publication

A focused eval adapter shares ordinary instance/static parent publication through
the existing property setters and their declaration/visibility checks. Empty null
parents become visible before an original-key failure. False parents publish before
the contextual deprecation dispatcher calls user handlers. The suspended write uses
an independent captured value and preserves an ordinary slot replaced by a callback.
Backing-slot selection is independent of an optional type hint and excludes accessor
hooks and attached native storage.

The shared PHP fixture checks both locations, null and false parents, original
integer/bool/array/object keys, callback replacement and in-place writes. Both staged
walker/VM engines and PHP/exact policies retire every owner. Native AOT behavior was
already correct, so it is retained as an executable and five-target emitter control.
ArrayKeys remains staged; explicit reference aliases, dynamic property slots and
pending warning-handler throw completion remain part of the following boundary audit.

All five focused gates pass: native AOT policy/optimizer controls in2.97s, all five
target emitters in1.30s, staged interpreter engines and complete owner retirement
in0.28s, real opaque eval with both optimizers and clean heap in64.70s, and default
compiler/bridge builds in35.44s. No new compiler warnings; diff hygiene passes.

### Verified171: parent value identity through coordinated references

The eval runtime contract now exposes a retained current value cell independently
from a mutable reference wrapper. The native v1 adapter uses target-aware ABI
helpers to dereference and retain that cell without making a PHP value copy.
FakeOps models the same ownership transfer. Property parent suspension checks pin
and compare those child identities, preserving callback replacements when Casts
or Coercions coordinates persistent property references.

The shared fixture now retains explicit aliases through a by-reference helper and
checks callback replacement, in-place mutation and the no-mutation control. This
uses syntax supported by both existing source parsers. PHP confirms the handler's
replacement survives even while the original property has another reference owner.
Staged engines also exercise ArrayKeys and Casts together, not just isolated flags.
The standalone pin regression distinguishes equal-valued array replacements while
holding the earlier cell alive. Public ArrayKeys admission remains closed.

All seven focused gates pass: native AOT policy/optimizer cases in2.81s, five-target
program emission in1.44s and value-pin ABI emission in0.26s, eight staged profile/VM
combinations with owner retirement in0.43s, the equal-array pin regression in0.14s,
and four real opaque-eval programs (Casts on/off and both optimizers) with clean
heap in112.73s. Default builds pass in39.43s with one Cargo job after the prior
parallel link received an unexplained SIGKILL. No new compiler warnings; diff and
assembly-comment hygiene pass. Callback unsets and pending handler exceptions
remain explicitly scheduled before ArrayKeys admission.

### Verified172: captured parent stores and escaping static references

Plain eval property stores now mutate the published value cell selected before the
false-conversion callback. They complete against that retained cell without replaying
an unset or rebound parent slot. Surviving aliases observe the completed write while
removed properties remain absent. Compound updates retain their separate snapshot
and attachment checks; post-callback probes use quiet presence checks.

The strengthened fixture retains an actual alias in an escaping closure and reads
it back for instance/static parents, replacements, in-place changes and unsets.
Fake reference sharing now traverses hidden closure object edges as well as visible
container fields, matching the native GC contract without phantom activation owners.

The alias readback exposed two static paths. Eval StaticProperty parameter captures
now escape through their owned formal cell and publish that same cell into static
alias metadata. This removes the retired-scope Variable target seen in the symbolized
SIGSEGV stack (PID1451676). Native ordinary Mixed static arguments now use live boxed
reference storage without requiring Casts/Coercions; other reviewed payloads keep
those gates. The managed reference authentication rules remain intact.

All seven focused gates pass: eight staged profile/engine cases with full retirement
in0.50s, hidden-child owner regression in0.14s, native PHP/exact optimizer cases
in4.63s, five-target emission in2.23s, four real eval programs with clean heap
in118.41s, default serialized compiler/bridge builds in112.99s and the existing
five-target declared static reference payload control in0.79s. No new compiler
warnings; diff hygiene passes. ArrayKeys remains staged while local/global unset
completion, compound contexts, pending callback throws and the other tracked
producer/source/owner boundaries continue.

### Verified173: pending conversion errors and shared boxed append completion

Plain eval writes now park a false-conversion handler Throwable while completing
only PHP's eligible original-key paths. Int, string and bool dimensions and
appends finish before rethrow; null and float dimensions stop, and array/object
key errors retain the callback exception in their previous chain. A shared
completion adapter owns the parked exception. The versioned protected native
Throwable-chain bridge borrows both boxed inputs and delegates identity, existing
links and cycle prevention to the established target-aware runtime helper.

Ordinary local and property writes share empty-parent publication. Captured
plain local writes mutate the selected current cell, preserving aliases without
replaying a callback's replacement binding. Compound local updates publish their
empty parent before immediate callback failure. Fake Throwable construction and
chain lookup now model existing previous links and borrowed graph edges.

The native controls exposed two older direct boxed paths: appends skipped false
conversion and compound assignments converted only during their final plain
store. Direct boxed appends now reuse the captured nested append kernel with
caller-owned operand protection. Boxed local, instance and static compounds use
the existing attached nested-update protocol, so pending conversion exceptions
stop their storage. Typed array and native object paths keep their established
selection boundaries.

All eighteen focused gates pass. Forty PHP8.5.12 oracle cases verify conversion
completion and exact Throwable identity. Eight staged profile/engine cases pass
in0.56s, five-target program emission in1.88s, PHP/exact native programs with both
optimizers in3.82s and four opaque native eval programs in122.63s with clean heaps.
The protected chaining ABI covers every target. Existing parent publication,
reference/string append ownership, ArrayAccess/SPL, COW and compound evaluation
order controls pass; the default serial compiler build passes in165.15s. Only the
inherited compiler warnings remain. No full suite, push or PR was run.

ArrayKeys remains staged pending its remaining producer/source/ownership and
native strict-eval admission gates. GLOBALS indexed unset is an unsupported
baseline PHP capability tracked separately in taske2e5a1f1; this step does not
claim a positive fixture for that missing surface.

### Verified174: copied reference keys and interpreter lookup ownership

Generic ArrayElement and NestedArrayElement targets now own typed key data,
not independently owned runtime key handles. Shared snapshot, materialization,
lookup and writeback helpers keep integer/string distinctions and binary bytes,
root temporary keys and retire them on every exit. Selection captures the key
before lookup callbacks. Equality between argument targets compares the captured
data directly. Abandoned or nonescaping targets retain their receiver cleanup;
cloning escaping metadata no longer duplicates an untracked key obligation.

The original regression retained a literal String("0") owner after a successful
call. Owned literal, integer and NUL-containing keys now survive generic method
calls and promoted-property aliases with full retirement. Nested target reads
also retire their intermediate array results, and property/static reference
binding leases their lookup results before validation or publication. Shared
statement value leases now publish their actual owner slot across native calls.

By-reference array reads preserve an already stored reference cell after the
same original-key guards and alias lookup. Array-backed parameter captures reuse
their owned formal cell under coordinated property policies, avoiding an earlier
UnsupportedConstruct with Casts. The existing key fixture now uses owned literal
arguments instead of unrelated key variables to keep operands alive. Selected
float keys snapshot without repeating their conversion diagnostic, while ordinary
metadata lookup keeps its established diagnostic behavior; resource identities
can also be captured as normalized integer data.

All twenty-two focused gates pass, including eight staged profile/engine cases,
four argument-failure cases, copied-key lifetime controls, existing reference,
COW, conversion-failure and parent-publication controls, five-target emission,
and four real opaque eval programs with clean heaps in121.38s. The PHP8.5.12
oracle agrees. The complete builtin exporter/generator/audit workflow passes
without generated changes, and the default compiler build passes in34.96s.

Two baseline controls remain explicit admission work. The existing settype
collection test fails identically on exact signed173 (journal e7aeaa47). A native
promoted-reference constructor fixture also retains five blocks after function
exit and an explicit collection, reproduced with the pre-step174 compiler
(journal6b64af5a). The full typed fixture passes with clean ownership in native
eval; AOT retirement requires its dedicated repair. ArrayKeys remains staged,
and neither baseline failure is reported as a passing product gate.

### Verified175: refreshed boxed receiver ownership

The native retained-hash control is repaired. A later nested write can widen the
final frame slot to Mixed, so an earlier concrete LoadLocal already carries an
unboxed owner. Refreshing that receiver previously acquired a second lease and
overwrote the first. The shared ReceiverPlace refresh now retires the old concrete
lease, while borrowed ref-cell views keep their existing ownership rules. The
packed Mixed-entry conversion chain uses an already refreshed entry helper after
publication, avoiding a second retirement of a transferred owner.

The original full PHP/exact-key fixture passes with clean heaps with both
optimizer settings. Two additional packed/typed receiver controls preserve their
by-value copies, reference updates and clean heaps with both optimizers, and emit
on every supported target. The full exact-key fixture also emits on all five
targets. The default serial compiler build, assembly-comment check and diff
hygiene pass. Journal e0ccd124 records the GDB evidence; e416bbf1 records the
minimal nested-write isolation. This supersedes the earlier constructor-owner
hypothesis: heap kind3 denotes hashes, and object cleanup was already balanced.

ArrayKeys admission remains staged. The preexisting checked settype collection
failure and Casts-off escaped array-parameter lifetime control remain active work,
followed by the outstanding ArrayKeys admission gates and other strict families.

### Verified176: independent collection snapshots for settype

Scalar array casts now transfer a cloned, dereferenced PHP value into element
zero on both architectures. Checked object construction leases a copied scalar
before storing its scalar property; existing objects copy their value cell while
preserving object identity. The fake array cast models the same snapshot and
transfers the new child owner into the array graph. This prevents a converted
collection from retaining the reference receiver that it subsequently replaces.

Valid array, object and null targets also use the shared conversion helpers in
permissive eval. The existing invalid-target false-result control is unchanged.
The shared PHP fixture checks scalar references, null, object identity, numeric
property names, array element updates and an independent copy. The original
native baseline printed Array instead of item and then failed while formatting
a self-referential scalar property (journal6077cbcf, extending e7aeaa47).

The interpreter control passes with Casts on and off and no retained activation
owners. Four opaque native eval programs pass with exact PHP8.5.12 output and
clean heaps in145.83s. The focused AOT scalar and existing collection controls
cover all supported targets and native optimizer modes. All nineteen required
gates pass, including the builtin exporter/generator/audit workflow, serial default
build and assembly/diff hygiene. No builtin signature is changed.

The full eval fixture additionally exposed an AOT limitation: applying settype
directly to an array element can pass checking and then panic during lowering.
Task29118b0b records that separate place-lowering/diagnostic work; AOT target
coverage here uses supported mixed local/reference storage. ArrayKeys remains
staged while escaped source lifetimes and its remaining admission gates continue.

### Verified177: persistent property reference sources in every profile

Instance and static property reference binding share owned_property_reference_source.
It attempts the existing persistent-cell promotion independently of Casts and
Coercions, so an escaped promoted array parameter uses its owned formal cell
instead of retaining a raw caller-scope target. Checked profiles still require
that capability; providers without a persistent result retain the existing
permissive fallback. Managed-reference authentication, foreign-slot handling and
the root classifier are unchanged. Existing local contract registration stays
inside the reused promotion helper.

The shared PHP fixture returns two aliases from scalar, array and nested array
factories, then reads and writes them after factory teardown. PHP8.5.12 agrees
with every expected observation. Sixteen staged walker/VM cases independently
select ArrayKeys, Casts and Coercions and retire all activation owners. Four real
opaque eval programs pass with clean heaps across Casts and optimizer settings
in144.87s. Every supported target emits the factory fixture. Existing owned-key,
persistent alias, by-value staging, checked property capture and static/nested
property controls pass. All ten gates, including the default serial build and
diff hygiene, are accepted.

ArrayKeys remains staged while producer/source, host/cache/mode and the disabled
strict native eval admission gate are completed. Stronger Locals and the other
five pending families remain part of the authorized implementation.

### Verified178: public exact array key admission

ArrayKeys is the eleventh admitted guarantee. Public checker, CLI and native eval
entry points now accept the independently selected profile. String keys preserve
their bytes and kind; integer keys remain distinct. The formerly ignored strict
native eval gate is enabled, and native builtin producer coverage includes the
exact profile. A new producer fixture covers JSON numeric names and serialized
keys with explicit string/integer kinds. The complete CLI example and reference
document the feature and the existing unresolved-Reflection restriction.

Producer integration exposed a retained converted unserialize wire. Both native
wrappers now publish shared owned-value exception guards for that wire and the
parsed result, then retire the conversion after context teardown. The unary
runtime builtin dispatcher also needed protection beneath Rust: its former direct
call let warning-handler ValueError escape the interpreter catch. A small shared
adapter now uses the existing PendingThrowable status and protected-call helper.
Facts54c9e03b and the unserialize-boundary entry record these observed failures.
Malformed strings, a mixed integer formatting input and throwing warning handlers
now preserve both control flow and ownership in native/direct and opaque eval.

All twenty-seven focused gates pass. Public PHP/LFC checking covers all five
targets. Native strict guards, builtin producers, original-source/folding and
Reflection controls pass with balanced heaps. The extended producer matrix passes
in68.79s. Clang assembles the complete eval runtime on Linux x86_64/AArch64, macOS,
iOS device and Simulator. Cache identity, immutable FFI profiles, twenty successful
CLI/profile/mode/cache cases and four original-key refusals pass. Explicit overrides
work with the umbrella in either order, and declare(strict_types=0) preserves the
selected semantic profile. The builtin exporter, all generators/audits, assembly
hygiene and restored default compiler build pass without generated-doc changes.

Nullability, Mixed, Initialization, ArrayTypes and Indexing remain unadmitted.
Stronger Locals binding joins/unset and the signed157 foreach/unset crash remain
active work before complete umbrella acceptance. The AOT settype array-element
limitation remains tracked separately as task29118b0b.

### Verified179: unset foreach aliases promote canonical boxed null

The signed157 foreach/unset crash still reproduced on signed178. GDB showed a
correctly detached raw local containing NULL_SENTINEL being promoted into a Mixed
reference cell without boxing. The next displaced-binding read dereferenced the
sentinel. Shared native promotion now canonicalizes only absent Mixed slots to
owned boxed null before the existing retain/release sequence. The new owner is
installed in the raw slot so promotion retires it normally. Native scalar payloads,
live boxed values and existing reference authentication retain their contracts.

A tracked regression covers repeated nested foreach references, rejected original
key kinds, exact numeric-looking strings, copy-on-write and throwing key evaluation.
All five supported targets emit it. Native permissive/exact profiles with both IR
optimizer settings pass with clean heaps. Existing loop-capture and conditional
unset representation controls pass. The serial default build, assembly-comment
hygiene and diff checks pass. Public CLI repeats all four crash cases and checks
supported closure promotion of Mixed null/live values, integer zero and the
sentinel-shaped integer in both optimizer modes, all with balanced ownership.
All eight focused gates are accepted. Fact2e16a69a records the promotion cause.

An extra CLI control found an existing checker refusal for by-reference closure
capture immediately after unset (Undefined variable in use()). Reinitializing the
local first succeeds. That refusal is not claimed as fixed by this promotion repair;
include it when checking initialization and binding-creation exceptions.

### Verified180: first-path native local binding origins

Ordinary inferred frame locals now preserve the producer that actually creates the
binding, including exclusive branches, optional births, numeric/null alternatives,
independent copies and legitimate unset/rebirth. The checker reuses its conditional
flow snapshots to merge possible source contracts for diagnostics. Known impossible
writes still fail statically; uncertain writes use the existing authenticated
reference coordinator. Mixed physical storage does not create an open contract.
Indexed per-scope/name origin metadata avoids repeated whole-program scans.

One local_binding_origins helper owns first-producer registration, independent copy
metadata and origin retirement. Typed EIR start/end operations share the existing
native/interpreter scope coordinators. Their runtime adapter respects both scope
layouts. Physical reset after unset cannot recreate source presence, and reference
storage detaches before a throwing displaced-object destructor can interrupt cleanup.
The observed failures and fixes are recorded in facts87ea6880 and5dd9b3bb.

All thirty-three focused gates pass, including native PHP/LFC alternatives with both
optimizer settings, core/interpreter scope layouts, throwing-destructor ownership,
all-five-target emission and complete runtime assembly, staged walker/dispatcher and
primitive VM execution, thirteen directly affected native controls, two error controls,
four EIR controls, the default serial build, assembly comments and diff hygiene.
Public CLI cases verify independent flags, explicit overrides in either order,
PHP-only policy, the admitted umbrella subset, source refusal within functions and
new binding contracts after repeated unset. All successful native cases have clean
heaps. The example and CLI reference describe the actual first-path contract.

The complete CLI probe additionally exposed a pre-existing early source transformation:
autoload rule collection flattens every constant top-level If before the checker.
Thus top-level dead-branch refusal is not included in the accepted source-refusal gate;
function-local source checks are covered. Correct that source preservation next,
without weakening or duplicating the semantic checker. Eval-containing bodies,
pre-bound/exposed storage and stronger Locals joins still require their own audit.
Nullability, Mixed, Initialization, ArrayTypes and Indexing remain unadmitted.

### Verified181: original binding source survives autoload discovery

Strict-local compilation now separates the executable autoload collection view
from the original source AST. A shared branch selector drives both views. The
collector reports consumed physical statement identities, and a focused source
helper removes only loader setup while preserving ordinary branch statements,
profiles, spans and control flow for the existing checker. Inactive setup does not
register rules, and its consumed sources are restricted to inactive spans so an
outer live callback is retained. Nested runtime conditions inside an unreachable
guard are handled without executing or registering their callbacks. The permissive
compatibility entry point retains its previous branch selection behavior.

The pipeline and test-suite registry pass the immutable semantic profile into
collection. Three focused library gates verify six invalid source shapes on all
five targets, selected and inactive loaders, nested guards, closure variables,
namespace shadowing and live outer callback sources. Two real CLI regressions
cover original-source refusals and class discovery/execution with optimizer on
and off. Both existing constant-guard loader execution controls still pass. The
default serial compiler build has no new warnings, and diff hygiene passes.
A further twelve public PHP/LFC cases reproduce eight expected source refusals
and four successful selected-loader executions with clean heaps. All ten gates
are accepted. Factf2634d3d records the earlier unconditional flattening cause.

The root is fixed without a second checker pass or disabling ordinary optimization.
This step does not admit another registered family; Nullability, Mixed,
Initialization, ArrayTypes and Indexing remain pending, along with stronger
Locals joins and legitimate reference-binding creation after unset.

### Verified182: legitimate fresh reference sources

A shared checker helper now initializes an undeclared reference source at creation,
with a null value and an open producer contract. Ordinary and contextual closure
captures use the same preparation; reference assignment shares it. Existing source
contracts and by-value capture diagnostics are retained. Named locals with explicit
declarations retain their prior handling. The helper records binding presence and
creation depth without running a deferred closure body.

Allowing those sources exposed stale Void lowering metadata: a fresh alias after
unset accepted a write of9, but reading the source still emitted null. Shared local
promotion now normalizes only frame-owned unbound Void storage to Mixed before cell formation.
Alias payload metadata is read after promotion, and self-aliasing forms a real
source cell before returning. Already bound incoming reference payload ABIs are
preserved. Fact55188904 records the observed failure. Old escaped aliases keep
independent closed contracts, and explicit null remains closed under Locals.

All ten focused gates pass. Both profiles lower on all five supported targets;
native optimizer on/off runs and walker/dispatcher execution produce matching
values with every owner released. The callback fixture uses contextual usort,
getter-only captures observe later stores, self-aliases are initialized, and
ordinary value captures still reject missing sources. Real opaque eval passes
both optimizer settings with clean native heaps. Existing foreach/promotion and
conditional reference-detachment controls pass. The default compiler build and
diff hygiene pass. Public PHP/LFC repeats all eight profile/optimizer combinations
with clean heaps and no undefined-variable warnings; the complete updated strict
example matches its tracked output and releases all owners.

The initial eval array_map callback segment exposed a separate permissive baseline
ownership defect. Taskb6efb927 records retained iteration keys and string leases;
removing that segment retires every owner. Its ordinary AOT path was clean, and
native opaque-eval impact is not claimed as measured. The final source-creation
fixture retains contextual callback coverage through usort. No builtin ownership
implementation is changed by this step.

A separate baseline probe still rejects an undeclared variable passed to a known
mixed reference parameter under Locals while permissive compilation accepts it.
Integrate reference argument creation with the shared call planner next, preserving
source order and ordinary-read diagnostics. Stronger Locals audits and the five
pending guarantees remain active before full umbrella acceptance.

### Verified183: discarded closures retain reference formation

Reference-capturing closure creation is now an observable AST effect. The deferred
body keeps its separate call effect, so creation does not execute or inherit the
body's output/throw behavior. By-value closure creation retains its previous pure
classification. DCE therefore preserves source binding formation even when the
closure result is discarded. The signed182 CLI printed no for an expected visible
name; Fact976f2b6e records why a successful null read with a clean heap missed it.

All eight focused gates pass. The structural optimizer regression proves reference
formation survives DCE without importing deferred body throws, while the by-value
control remains removable. All five targets emit the retained source. Native PHP/
LFC, both profiles and both optimizer settings preserve get_defined_vars visibility,
null initialization and a subsequent open string store, with clean heaps and no
undefined-variable warning. The complete fresh-reference native fixture remains
green. The serial default compiler build and diff checks pass. Eight public CLI
cases independently confirm the same scope observation across source modes and
optimizer/profile choices. Six further CLI probes preserve optional binding
presence in main and function scopes, including paths that never create it,
matching PHP with clean heaps. The CLI reference explains unused-closure formation.

Continue with call-planner reference arguments and the remaining stronger Locals
and pending-family audits. This fix admits no additional registered guarantee.

### Verified184: source-order reference arguments

CallArgPlan now projects directly supplied reference source indices, retaining
unpack provenance and defaults without using spans as identities. The checker
consumes the same planned source list for declared free functions and tracked
callable signatures, creates each missing plain local at its argument position,
and retains existing closed producers. Provisional signatures reuse declaration
metadata and leave authoritative call diagnostics with the ordinary validator.

Empty reference-taking functions, methods and closure invocations now retain the
caller binding effect in AST summaries independently of body effects. A baseline
optimized permissive call lost its newly formed name from get_defined_vars; the
shared parameter-binding effect fixes that regression without making by-value
calls impure. Fact4ccbbcde records the source-observable distinction.

The eval signature snapshot requests persistent cells for interpreted reference
parameters at their source operand. The initial walker probe missed a new name in
later get_defined_vars and read stale null after a later nested reference write.
The full walker and normal dispatcher fixture now agree with native behavior in
both profiles, with all Fake owners drained. Existing native reference layout
selection remains unchanged. Factea45e105 records this early identity requirement.

Verification covers planner indices and matching, same-span inputs, direct/named/
variadic/first-class/closure arguments, defaults private to the callee, source-order
reads and writes, catches, preserved closed origins, empty bodies, five-target
emission, native values and clean heaps, real opaque eval in both profiles and
optimizer settings, prior reference formation controls, pure-call controls, the
default serial compiler build, PHP/LFC CLI combinations and the complete updated
strict-locals example. All focused gates and diff hygiene pass. PHP confirms the
permissive fixture output. This step admits no additional strict guarantee.

### Verified185: known member reference sources and actual scope presence

The shared source-index projection now covers concrete instance/static methods,
interface and inherited signatures, constructors including new self, nullsafe
method arms and expression callees with authoritative callable signatures. Pure
metadata queries reuse existing class resolution without descriptor specialization.
Receiver effects precede argument selection, and each selected plain name is
created at its source position. Earlier ordinary reads still diagnose undefined
variables. Known callee validation retains its own diagnostics.

The extended nullsafe native fixture exposed a separate presence error: a skipped
reference argument appeared in get_defined_vars with a clean heap, even without
strict flags or AST optimization. The reference call itself remained in the correct
branch. Scope enumeration used compile-time initialized slots, and the nullsafe
continuation retained operand-only initialized facts. Fact588ddb63 records that
baseline. Frames containing direct or explicit call_user_func scope introspection
now opt into the existing native/eval provenance words. Enumeration guards each
candidate before loading it, preserving explicit null bindings and excluding
unexecuted/future names. A scoped AST visitor ignores deferred frames and borrows
the existing traversal without cloning syntax. Binding presence operations are
profile-independent infrastructure; strict producer operations retain their profile
gates. Nullsafe local joins also retain conservative initialized-slot facts.

Focused verification covers five-target emission and validation, native profile/
optimizer pairs with clean heaps, skipped/taken nullsafe receivers, interface and
inherited dispatch, constructors, immediate/empty closures, variadics, later
reference writes, old closed origins, explicit null and conditional scope names,
source-order diagnostics, scanner frame isolation, actual switch scope outputs,
walker/normal dispatcher parity with complete Fake owner retirement, real opaque
eval, prior free-reference and discarded-capture controls, nullable result storage,
the default serial build, PHP/LFC public CLI combinations and the complete updated
example. PHP confirms the permissive fixture. No additional strict guarantee is
admitted by this step.

### Verified186: first producers survive native reference exposure

Strict scalar frame origins now remain eligible when a call may expose the local
by reference. Foreign reference parameters, pre-bound storage, globals/statics,
aliased lvalue roots and unsupported representations keep their existing storage
exclusions. The old scanner rejected exclusive int/string first assignments as soon
as a reference-taking call appeared later in the body. New tests prove direct and
named calls, tracked first-class callables, instance/static methods and constructors
retain the executed first producer through accepted and rejected writes.

Calls whose parameter ABI is unresolved now distinguish reference exposure from
natural null-only storage. Their null-only sources remain unboxed for nullable
pointer inputs; the separate fact cannot disable strict origin tracking for
informative scalar alternatives. Pointer-by-reference sources still require the
existing explicit nullable-pointer declaration. The control covers a declared
reference source and an untyped null passed by value through a typed closure,
checks natural frame representation on all five targets, and executes it natively.
The direct staged LFC helper requires a PHP header even though public LFC source
does not; Fact672cbc3c records the test-harness distinction.

All twelve gates pass. Five-target strict emission and native optimizer pairs
preserve exact int/string and int/float origins, open Mixed producers, callee
side effects, constructor unwind and clean owners. Known impossible stores remain
diagnostics. Walker and normal dispatcher agree in both profiles and drain every
Fake owner. Previous ordinary-origin, free/member reference and diagnostic controls
remain green. The default serial build and diff hygiene pass. Four public strict
PHP/LFC optimizer cases and the complete reference-enabled example pass with clean
heaps. The unchanged permissive AOT scan still rejects this particular exclusive
reference-exposed source; ordinary accepted permissive reference fixtures remain
green, and the interpreter's permissive output agrees with PHP. This step changes
no permissive scan eligibility and admits no additional strict feature.

Conditional parameter unset and eval-containing unset remain active, alongside the
five pending guarantees. Plain straight-line parameter unset already works.

### Verified187: conditional recreation of by-value parameters

Eligible native parameters now retain their declared incoming contract until an
executed conditional unset starts a new local binding. The checker records entry
origins separately from assignment spans; repeated body walks replace those entry
facts. A focused entry_origins helper selects actual owned parameters, preserving
foreign aliases, raw layouts and trusted-source exclusions. Only selected inputs
acquire an owning boxed shadow after the contiguous ABI parameter prefix. The
existing producer-start path seeds that shadow and ordinary source stores establish
replacement contracts after an executed binding end. The input ABI remains intact.

Incoming storage and semantic contracts are distinct PreBoundLocal facts. Explicit
hints retain their declaration, unannotated parameters stay open, and variadic
collectors retain an array contract. Legacy storage replay, numeric regions and
promotable parameter analysis still receive the original physical types. The first
prototype incorrectly closed an inferred resource parameter; the eval comparison
and existing strict_parameter_locals test established the open-input rule. Closure
parameters now read their resolved environment instead of the enclosing scope;
a same-named outer string no longer supplies an int parameter's entry metadata.

All twelve gates pass. Five-target emission and native optimizer pairs cover
integers, nullable and numeric union parameters, explicit Mixed, unannotated
resource inputs, base-class/subclass acceptance and unrelated-class refusal,
arrays, methods, static methods, constructors, closure name collisions, variadics
and skipped/taken scope presence. Rejected stores retain old values. Caller arrays
remain unchanged, and the original resource remains writable and closes once.
Walker and normal dispatcher agree in both profiles and drain all Fake owners.
Known invalid stores remain diagnostics. Isolated unannotated scalar and open-only
scope programs preserve open inputs without depending on another typed origin.
The existing parameter policy regression, prior reference/nullable-pointer controls,
default serial build and diff hygiene pass. Four public strict PHP/LFC optimizer
cases and the complete updated example pass with clean heaps; that example also
exercises the linked interpreter context. Its conditional reset now uses an int
parameter rather than an ordinary local, without changing its output.

This step admits no additional guarantee. Eval-containing bodies, foreign reference
parameters, escaping aliases and remaining capture/global/static joins stay active,
alongside Nullability, Mixed, Initialization, ArrayTypes and Indexing.

### Verified188: owned binding ends in eval-visible frames

Owned fresh locals with a plain unset now retain runtime-origin metadata even when
the enclosing body contains eval. The scanner distinguishes outgoing captures from
foreign incoming references and selects only frame-owned, non-typed, non-global,
non-static sources. Binding tracking does not require an AST assignment: implicit
reference creation and single-producer names also retire their origins. Existing
binding-state and source-end EIR operations retire the name before physical cleanup;
old escaped cells retain their values and contracts. No runtime layout or assembly
emitter changed. Ordinary reference-capture marking remains unchanged outside this
admission, including the existing top-level fresh-reference control.

Focused all-five-target emission, optimizer on/off native execution, known-invalid
pre-eval diagnostics, both interpreter entries/profiles and owner retirement passed.
Conditional parameter and fresh-reference controls passed. Default compiler build
and diff hygiene passed. Public PHP/LFC optimizer pairs and the complete updated example passed with clean
heap diagnostics. The public permissive compiler still rejects the pre-existing captured
int-to-string eval-body shape; permissive interpreter parity and previously accepted
native fixtures remain controls. See step188-evidence.json for final acceptance.

### Verified189: owned incoming bindings in eval-visible frames

Recreatable by-value parameters now retain their declared/open entry origin even
when the native body contains eval. Both conditional and unconditional plain unsets
use the existing owning shadow rather than abandoning a slot that eval can address.
Incoming ABI slots remain unchanged. Outgoing old captures retain their own cells,
while new captures and eval-created values belong to the replacement binding.

The native integration fixture exposed an actual shadow-selection bug: the new
`#binding` suffix from187 was not understood by the authoritative eval inventory,
which synchronized the incoming ABI slot under the PHP name. After eval recreated
an array parameter as a string, the body still read the null sentinel in its shadow
and crashed in `__rt_mixed_unbox_loop`. Symbolized debugger evidence identified
`evalArrayParameter`; this was not resource exhaustion. Reusing the established
`#cow` convention makes flush/reload, storage replay and PHP-name consumers select
the same active slot. Diagnostic copying hooks were removed and saved executables
and assembly files cleaned. No new assembler strings or runtime layout were added.

All-five-target emission, native optimizer pairs, both interpreter entries/profiles,
known-invalid store diagnostics, conditional-parameter/fresh-reference controls and
the existing eval inventory test passed. The default compiler build passed. A real
opaque-eval test proves native and eval source origins, old aliases, rejected stores
and complete owner retirement with both optimizer modes. Public PHP/LFC optimizer
pairs and the complete updated example passed with clean heap diagnostics. See the final
step189-evidence.json for acceptance and gate timings.

### Verified190: conditional incoming reference origins

Incoming reference handles now retain logical entry contracts independently of the
foreign pointee. Conditional unset ends the local name without copying the caller
into a value shadow. Existing managed leases, runtime representation flags and
binding-origin helpers create a fresh local only on the executed detach path.
Open reference parameters preserve caller constraints, and explicit mixed callers
remain open until a new binding establishes a replacement contract.

The canonical binder skips unowned handles. Its payload comes from the current CFG
proof rather than the original declaration, avoiding two leaked null boxes when a
new Mixed cell replaces an integer reference. Headerless host reads retain the
incoming raw ABI when current payload proof is unavailable; the binder keeps that
fallback separate from actual fresh-cell evidence. A reusable native test helper
links C callers, checking attached writes, detached values, post-join stores,
string pairs and floating payloads with optimizer on/off and clean heap reports.
All supported targets emit these paths through the existing ABI helpers.

Interpreted parameter and capture writeback now share a live-target identity check,
so an unset/recreated name cannot overwrite the original argument merely because
it becomes a reference again. Both interpreter entries/profiles, original closure
and method writeback regressions and full owner retirement passed. A real opaque
source test exercises these interpreted cases and native incoming reference scopes.
That test exposed an absent-slot read in eval's numeric access-mask preflight:
a detached native parameter still held NULL_SENTINEL, and unboxing it crashed.
The preflight now uses the existing presence guard before taking any snapshot;
a focused all-five-target test checks that presence precedes retain and unbox.
Debug copying hooks and saved binaries/assembly were removed.

The focused validation includes all-five-target emitters, native optimizer pairs,
C callers, both eval entries/profiles, prior parameter/fresh-reference controls,
raw/canonical reference reader controls, known-invalid source diagnostics, opaque eval with a freshly rebuilt Magician
archive, default build, assembly comments, PHP/LFC optimizer pairs and the updated
complete example. Final acceptance is recorded in step190-evidence.json. No runtime
layout changed and no broad local suite, push or PR was requested.

### Verified191: open parameter contracts independent of inferred views

Strict local checking now seeds every explicit parameter with its declared or open
source contract before scanning body storage. An unannotated input remains open
when call-site inference narrows its checker entry view to int, float or a resource.
Ordinary stores update the flow view without inventing a reference cell; potentially
externally coerced aliases retain a conservative Mixed view. A real unset still
ends the incoming origin and lets the first fresh producer close the new binding.
Declared parameters and ordinary inferred locals remain constrained.

The existing EIR ABI already boxes ordinary untyped by-value inputs. A production
baseline on the preceding signed commit confirmed that distinction, so this change
does not widen the ABI or add owner shadows for ordinary stores. Existing origin
helpers were renamed to describe their shared binding responsibility. Raw pointer,
buffer and packed entry representations keep their explicit exclusions.

Focused regressions cover repeated call-site inference, methods, static methods,
closures, promoted constructor parameters, by-reference propagation, resources and
closed caller contracts. The example now demonstrates an untyped open input. The
new fixture runs on native optimizer pairs, both interpreter entries/profiles and
real opaque eval, with owner retirement checked, plus all-five-target emission,
known-invalid diagnostics and previous detached parameter controls. Public PHP/LFC
optimizer pairs and the complete example are required in step191-evidence.json,
along with the default build and hygiene checks, before acceptance.

### Verified192: captured binding ends preserve their incoming origins

Incoming capture metadata now separates the enclosing producer contract from the
capture snapshot layout. The checker saves these contracts while the enclosing
frame is active and seeds them in the closure body independently of narrowed
storage views. Shared entry-origin selection admits captured names with plain
unset, including unconditional ends which cannot use ordinary parameter slot kills.

The existing common function prefix initializes their owning value shadows or
retained reference handles. A captured reference keeps its external pointee until
the executed detach; the replacement stays local to that activation. Repeated
invocations preserve the original closure environment. An outgoing old reference
continues to observe its original cell, and known incompatible stores remain
checker errors both before a future end and after the first fresh producer.

Coverage includes both capture modes, conditional/unconditional unset, open Mixed
inputs, old aliases and repeated invocation. The focused gates require emission on
all five targets, native optimizer pairs, both eval entries/profiles with all owners
retired, actual opaque source with native captured value/reference scopes, existing
open-parameter/reference controls, the default build, assembly comments and public
PHP/LFC optimizer pairs. The complete example demonstrates detaching a captured
reference and retains its existing expected output. Acceptance evidence is stored
in step192-evidence.json before the signed commit.

### Verified193: outgoing reference captures retain executed producers

Strict storage selection now admits an owned conditional producer even when an
outgoing closure captures that name by reference. Strict native origins already
use canonical reference cells, so capture formation can share the executed source
without writing a boxed pointer through foreign raw storage. Incoming reference
parameters/captures remain excluded by their independent ABI role, and permissive
storage selection keeps its existing behavior.

Focused tests cover both exclusive first producers, accepted/rejected subsequent
writes, propagation back to the enclosing frame, an escaped old alias after unset,
and an ordinary top-level captured integer. Known incompatible writes to a live
origin remain checker diagnostics. Both native optimizer modes and eval execution
entries/profiles retire every owner. Real opaque eval checks interpreted closures
and native captured scope writes, plus preceding capture-end, eval-binding-end and
reference controls. Acceptance additionally requires all-five-target emission,
default build, assembly comments, public PHP/LFC optimizer pairs and the complete
example, which now writes through a reference-capturing closure with unchanged
expected output. The signed result is backed by step193-evidence.json.

### Verified194: immutable producer descriptors travel with value captures

Closure snapshots now carry the enclosing runtime-selected producer descriptor as
an ordinary generated hidden capture. The descriptor borrows immutable program
metadata and survives retirement or rebinding of the source activation without
sharing its cell. Checker entry evidence distinguishes these captures from open
Mixed inputs, including read-only intermediate captures needed by nested closures.
Each activation uses the common entry-prefix and existing canonical binding-copy
operation to install an independent contract before publishing its value shadow.

The new typed EIR descriptor read reuses the existing scope registry ABI on every
target. Binding-copy lowering accepts an optional frozen descriptor and normalizes
owned #cow storage names back to their PHP names. Explicit open descriptors remain
open. Validator checks require StrictLocals and typed descriptor operands; shared
ABI helpers handle all emission without changing closure headers or reference-cell
layouts. Generated names keep descriptor values outside PHP scope enumeration.

The fixture checks rejected and accepted writes for both executed first producers,
unchanged enclosing values, escaped factories, repeated activations, nested closures,
call_user_func and call_user_func_array, open producer alternatives and hidden-name
filtering. Native optimizer pairs and both eval entries/profiles retire every owner.
Extended acceptance in step194-evidence.json requires real opaque eval against both
interpreted and native captured scopes, previous reference/capture/copy controls,
all-five-target emission and validator negatives, interleaved hidden descriptors and
string pairs spilling to the stack through callable descriptors, assembly comments, the default
build and public PHP/LFC optimizer pairs. The example and its expected-output test
show independent snapshot values and source values. No broad suite, push or PR.

### Verified195: detached foreach values retire before the next reference binding

Reference iteration now releases an owned raw value left by an earlier detach before
replacing the local with the next array-element reference. The existing displaced
pin preserves its payload until publication, while the existing runtime state test
skips attached references. This reuses ReleaseRawLocalSlot and changes no cell or
iterator layout. Baseline CLI probes leaked one 24-byte string and one 40-byte
Mixed cell when the first replacement was overwritten on the next iteration.

Focused tests cover unconditional and conditional unset/recreation, preserved source
array contents and ordinary attached reference loops. Both optimizer modes and both
permissive/StrictLocals profiles now retire every owner; all five targets emit the
same path. The default build, assembly-comment check and diff hygiene pass. Eight
public PHP/LFC profile/optimizer combinations and the complete example are clean.

The added interpreter execution probe exposed an existing language-subset boundary:
EvalStmt::Foreach has no reference role and the parser rejects reference targets.
The native memory fix does not extend that language subset. The unsupported forms
are now documented and tested as parse errors in both profiles, with implementation
tracked separately as task a8e94959. No parser behavior or serialized cache shape
changed. Acceptance is recorded in step195-evidence.json before the signed commit.

### Verified196: global value snapshots borrow the program-owned source

Ordinary tracked LoadGlobal expressions now explicitly borrow their symbol's heap
value, like reference-cell reads, instead of leaving provisional MaybeOwned
metadata on a direct pointer load. Closure capture takes its own lease and its
view cleanup no longer consumes the program symbol's owner. Explicit storage
loads for retirement keep their separate Owned path. No global representation,
reference layout or target ABI changed.

The baseline snapshot of global integer 1 printed Array:2 after the symbol was
replaced with 2, in both profiles, despite a clean heap report. The new regression
retains captured integers, strings, arrays and objects after source replacement.
Acceptance requires borrowed-load emission on all five targets, native profile and
optimizer pairs, both interpreter entries/profiles with complete owner retirement,
existing captured-value controls, the default build and public PHP/LFC combinations
plus the updated complete example. Evidence is recorded in step196-evidence.json
before committing. Global alias unset and reference rebinding remain separate active
work; this ownership correction does not claim to resolve them.

### Verified197: conditional native static binding origins

Conditional and eval-visible static binding ends now preserve the successful initializer's producer independently of activation-local replacements. The existing per-body storage scan selects these already-canonical static names and retains displaced parameter/capture entry contracts. A shared declaration-contract recorder uses explicit producer evidence instead of importing the name's earlier runtime origin. Reached declarations mark binding presence after initialization, retire the displaced origin, bind the persistent cell and publish the new activation origin through existing reference/state helpers.

Coverage includes scalar and array statics, an older open parameter, escaped old reference captures, runtime membership rejection, literal eval and known-invalid stores. Native output and heap owners are checked with both optimizer settings; real opaque eval checks attached and detached paths. The interpreter fixture uses both entries and profiles, retiring its persistent context owners before the final zero-owner assertion. This corrects test teardown rather than changing interpreter static semantics. Existing storage and unconditional-detachment controls remain covered. Public PHP/LFC cases with both optimizer settings exercise the strict behavior and preserve the existing no-flag retyping diagnostic. Native permissive storage still excludes statics from generic branch widening; the new selector is gated on Locals. The complete strict-locals example exercises the conditional static behavior. Documentation explains persistent values and activation bindings separately.

No additional semantic family is admitted. Native global roles and Nullability, Mixed, Initialization, ArrayTypes and Indexing remain active work.

All 12 acceptance gates passed. The public controls include four strict clean-heap runs, four preserved legacy diagnostics and the complete example.

### Verified198: independent global activation leases

Ordinary function globals now acquire an owned managed reference cell through typed LoadGlobalRefCell and guarded direct-producer adoption. The shared persistent-cell binder preserves earlier raw frame layouts. Unset and reference reassociation detach only the activation name; escaped references keep the program value, while value captures retain independent snapshots. Main, request superglobals and extern storage retain their existing boundaries. GLOBALS projection reads program symbols rather than detached local replacements.

Mixed program reads dereference wrappers. Program stores and eval reload share swap_mixed_symbol, which publishes through an existing wrapper and prevents self-child publication before retiring the displaced owner. StoreGlobal declares PHP callback effects. Reached global statements are observable and may throw: retiring an older local precedes alias publication, while replacing a program value publishes the new value before retirement. The interpreter uses the same two orders and ends a named global alias before reference reassociation.

Core analysis exposed a raw integer cell accessed using a stale Mixed payload after reassociation. Reference writes now consume current proven payload facts, retaining the instruction ABI only when proof is unavailable. Reference-bound facts are restored per branch and unioned across reachable predecessors, separately from definite initialization. The fixture covers integer, float and string reassociation, conditional detach, reference/value captures, arrays, objects, missing globals, explicit mixed reference updates, and caught destructor failures for both declaration and program publication.

Acceptance covers a default compiler/Magician archive build, exact owned-transfer and borrowed-copy validator tests, all five emitters, native and real opaque-eval execution with both profiles and optimizer settings, both interpreter entries with zero retained owners, directly affected reference/global/static/foreach controls, assembly comments and diff hygiene. Public PHP and LFC cases exercise all eight profile/optimizer combinations with clean heaps; the complete strict-locals example also passes. A fixture helper was moved before use after a staged-entry undefined-function error, without weakening the mixed reference case.

No additional semantic family is admitted. Global producer-origin enforcement, Nullability, Mixed, Initialization, ArrayTypes, Indexing and complete umbrella acceptance remain active.

All 19 acceptance gates passed in job e0987ad4. PHP 8.5.12 produces the same complete fixture output; every public executable reports a clean heap.

### Verified199: complete source-owner retirement at global publication

Program-global publication now parks its separate source lease through existing unwind-visible operand helpers before StoreGlobal can retire a throwing old value. The published owner remains independent. Main producer metadata allocates the EvalGlobalScope slot already selected by codegen, and shared validation requires exactly that owner for main or EvalScope for function activations. Scope start, bind, end, query and copy instructions consume the same rule. A negative validator test exercises an actual binding instruction with the wrong owner kind.

The new fixture publishes an object, array and runtime-produced string through a program symbol observed by an escaped reference. Every old value throws during destruction. The native baseline printed the right object but retained40bytes with refs1; the corrected path preserves PHP's DCobject:2|DCarray:3|DCstring:xx| and balances source owners. The fixture retains its global-declaring function through an actual call/capture, and the emitter assertion requires StoreGlobal, so ordinary frame storage cannot silently replace this acceptance surface.

Interpreter acceptance exposed two separate problems. FakeOps final destruction ignored live wrapper/container edges; it now checks the existing physical-owner graph for both the released handle and its projected object, with an independent snapshot regression. After fixing that premature destruction, a real array source lease remained because retirement loops stopped at the first exception. Shared retire_scope_replacements roots actual RuntimeOwnerSlot fields, takes each before PHP cleanup and retires all leases while preserving failure. Walker statements, assignment expressions, the VM and the top-level static fallback reuse it. The shared fixture reporter exposes pending Throwable messages for failures; temporary diagnostic markers were removed.

Focused acceptance refreshes the compiler and Magician archive, covers all five emitters, native and real opaque-eval execution with both profiles and optimizer settings, both interpreter entries with zero retained owners, the selected-scope validator and physical-owner regression, directly affected global/capture/static controls, diff hygiene, eight public PHP/LFC/profile/optimizer cases and the complete strict-locals example. PHP supplies the fixture oracle. The unrelated pre-existing ordinary frame retirement order remains tracked as task4471067e; this step does not claim to repair it.

No additional strict family is admitted. Reached global producer roles and conditional recreation remain active, followed by Nullability, Mixed, Initialization, ArrayTypes, Indexing and complete umbrella acceptance.

All 14 acceptance gates passed in job b76365ef. The refreshed compiler/archive, four real opaque executions, eight public combinations and the complete example all pass, with clean heap/owner assertions.

### Verified200: reached global views replace old activation facts

Global statement checking now lives in the focused assignments/global_bindings.rs module. A reached declaration replaces the activation's old environment view with the available program-source view, or Mixed for an unseen source. It clears stale local, callable and reference metadata plus displaced incoming/static roles, marks the new alias present and retains retirement exception tracking. It does not infer broader reference authorization from canonical storage.

The baseline rejected a valid string program source as an int-to-string retype because an int parameter of the same name remained in the environment. The new shared fixture covers parameters passed by value, foreign int references whose callers remain1, ordinary locals, persistent statics across repeated calls, and declared mixed program sources whose displaced parameters were concrete. The inverse regression refuses a string store to an integer program source even when the displaced parameter was string.

Acceptance covers the refreshed compiler/Magician archive, all five emitters and negative checker targets, native profiles/optimizers, both interpreter entries with zero retained owners, real opaque eval, directly affected existing global and publication controls, diff hygiene, eight public PHP/LFC/profile/optimizer combinations and the complete strict-locals example. PHP supplies the fixture oracle. Physical global leases and raw parameter ABIs retain the previously verified helpers.

This step repairs reached source views. Original program producer contracts across narrowed/replaced views and conditional global binding ends remain active work. Nullability, Mixed, Initialization, ArrayTypes, Indexing and complete umbrella acceptance remain unadmitted.

All 11 acceptance gates passed in job 6dc015b7. Every public executable and the complete example reports a clean heap.

### Verified201: conditional global activation origins

The existing persistent declaration-origin selector now covers ordinary reached function globals as well as statics. It preserves displaced parameter/capture entry evidence and excludes trusted declarations, request superglobals, extern globals and this from the new global selection. Runtime local-origin checking admits an active global only with that explicit proof. Reached declarations record their producer independently of displaced entry facts, and native global leases reuse end, bind_persistent_ref_cell_ptr and start_reference_declaration. No global sidecar, cell layout or new origin protocol was introduced.

An executed conditional unset ends the activation's binding. Its replacement gets the successful producer's contract; a still-attached predecessor retains the program contract. Re-declaring global reconnects to the persistent value. The shared fixture covers old reference captures, mixed replacement producers, integer/float rejection, allowed integer updates, a fresh string producer rejecting an integer, and repeated declarations after detachment. The expanded strict-locals example shows a detached string label and rejection through the still-attached integer global.

All 13 gates passed in job ee186713: refreshed compiler/archive, all five emitters, both optimizer native and opaque-eval runs, both interpreter entries/profiles with zero retained owners, the preserved native no-flag cross-kind diagnostic, existing static/parameter/raw-reference/reached-global controls, diff hygiene and public PHP/LFC cases. Four strict public runs and the complete example have clean heaps; four no-flag controls preserve the legacy diagnostic. Earlier initial probes also passed all-target/native/interpreter checks. Separate frontend probes202 accept explicit mixed and union program producers after a root write, so no missing original-producer behavior is inferred solely from that scenario. Their executable positive and negative boundary checks remain next.

No additional strict family is admitted. Nullability, Mixed, Initialization, ArrayTypes, Indexing and complete umbrella/host/cache/optimizer acceptance remain active.

### Verified202: executable original global producer boundaries

Regression coverage verifies existing original program contracts after earlier root stores. An explicit mixed producer stays open; int|string accepts both alternatives and rejects float; ?int accepts null/int and rejects float; a concrete integer global rejects mixed float/string candidates without changing its value. The distinct permissive goldens preserve PHP's dynamic writes. The shared fixture uses independent global/function names so every source remains observable.

All seven acceptance gates passed in job df72e7c2: incremental compiler/archive build, all five emitters under both profiles, four native optimizer/profile runs, both interpreter entries/profiles with zero retained owners, four real opaque-eval executions, diff hygiene, eight public PHP/LFC combinations and the complete example. All native/public runs report clean heaps. The preceding eight isolated executable probes1c15ffe2 also passed. No production origin map, global sidecar, registry or ABI change was necessary for these measured boundaries.

The remaining implementation now proceeds to Nullability, Mixed, Initialization, ArrayTypes and Indexing, then complete umbrella/host/cache/optimizer/docs/example acceptance. Eleven features remain integrated; the full umbrella remains gated.

### Verified203: initialized native frame reads and quiet probes

User frame reads now test the existing native/eval binding-presence words before observing storage. Parameters, value captures and loop-carried slots share the guard; statically absent ordinary reads receive the shared strict-initialization diagnostic. Initialized null remains present. The optimizer conservatively retains potentially throwing ordinary reads even when their values are discarded.

Simple-variable isset, empty and coalescing test presence before payload. Coalesce snapshots preserve the original value type plus null, including typed integer returns, bool, float, string and array values. Native pointer, buffer and packed storage retain their dedicated paths. Physical unset cleanup cannot recreate binding presence, independently of whether strict-locals recorded a producer origin. Conditional detachment of raw-reference parameters and value captures preserves caller/outer values.

Eight acceptance gates cover the refreshed compiler/archive, all five target emitters, PHP and LFC native runs under both optimizer settings, clean heaps, known-missing diagnostics, uncaught Error metadata, existing conditional-parameter and raw-reference ABI controls, and diff hygiene. The earlier expanded regression failed on detached parameter/capture reads before the presence retirement fix; the corrected runs pass. No runtime layout or new opcode was introduced.

Initialization remains staged and publicly rejected while interpreter, eval/global scopes, nested quiet roots and property read integration continue. No additional family is admitted. Nullability, Mixed, ArrayTypes, Indexing and complete umbrella acceptance remain active.

### Verified204: shared interpreter missing-read policy

Walker LoadVar and bytecode load_local share one missing-variable helper after visible binding lookup. Initialization throws the same catchable Error as native frame guards; permissive execution retains PHP warnings and null. Initialized null and false remain visible bindings. Quiet simple-variable isset/empty/coalescing continue through their presence-aware language paths.

Focused tests cover direct walker and VM reads under both profiles, exact messages and Error class, the complete native frame fixture through walker and internal dispatch, balanced scope/persistent owners, PHP/LFC native optimizer parity, the existing VM local-read arithmetic regression and public rejection of every catalogued pending guarantee. The latter test previously pinned Casts, which is already integrated; it now derives pending features from the shared catalog. Seven acceptance gates pass. An additional failed-operand release-injection control reproduces its assertion with unchanged HEAD walker/VM code, so it is tracked separately rather than counted as a passing gate.

The fixture's fallback counter now uses a declared by-reference helper with a statement increment: the eval parser supports local updates as statements but refuses local prefix increments in expression position. This preserves the fallback-once oracle. A public dispatch call in the staged test initially encountered the intended pending-profile gate, so the test now enters internal dispatch/fallback explicitly. Real opaque eval remains deferred until complete Initialization admission; no host gate was weakened and no partial family was advertised.

Initialization global/eval scopes, nested quiet roots and property read boundaries remain active, followed by Nullability, Mixed, ArrayTypes, Indexing and complete umbrella acceptance.

All seven gates passed in job9f14a7b9. Baseline jobcf191dbc confirms the separate release-injection assertion, tracked as taskb1954cd6.

### Verified205: canonical global presence and named property initialization

Initialization now observes canonical program-symbol presence independently of frame allocation, using the read-only Boolean GlobalBindingPresent EIR primitive and shared target ABI helpers. The validator rejects non-Boolean results with internally consistent value metadata. Process entry publishes argc/argv presence before source execution. Existing frame presence remains usable across eval barriers; reached static and function-global activation leases preserve their caller and persistent values after detachment.

Named ordinary property reads carry existing ProfiledData source policy through lowering and inlining. Backend missing-property paths throw the shared initialization Error only for eligible user reads. Initialized null remains valid. A focused initialization_properties helper selects an accessible untyped fixed slot or its magic accessor through PropInitialized, pins receivers with existing call-operand owners and joins through ExprBranchJoin. Typed properties, nullable receiver dispatch and property hooks keep their existing paths. Quiet probes distinguish __isset-only isset from empty/coalescing value fetches, including coalescing through __get without __isset. Counter regressions verify exact calls.

Interpreter probe_reads shares quiet receiver-chain evaluation with ordinary key evaluation and existing owner finishers. isset, empty and coalescing no longer treat an absent array root as an ordinary missing-variable read. Keys still run once and can throw Initialization errors even if a null receiver discards their value. Native evaluate_once now snapshots Initialization-checked variable operands before such branches. Shared interpreter property checks preserve initialized null and the same accessor protocol.

An expanded owner test exposed an existing FakeOps adapter error: missing array reads created null then retained it again. Existing elements still retain, but missing indexed/hash/non-array reads now transfer one fresh null owner. A dedicated regression checks the exact ownership count. All source phases, the complete fixture and both internal interpreter dispatch modes release every owner; native executions report clean heaps.

Fourteen final acceptance gates cover the refreshed compiler and Magician archive, five-target emission, PHP/LFC native optimizer runs, staged interpreter parity, uncaught property Error metadata, negative EIR shape validation, existing frame and global producer controls, direct walker/VM missing reads, adapter ownership, diff/assembly hygiene and the complete builtin documentation generation/audit workflow. No additional strict family is admitted. Program-global unset, eval-only scope/synchronization, dynamic/static/reflection/update reads and final Initialization public/host/cache/docs/example acceptance remain active, followed by the other four pending families and the complete umbrella.

All fourteen gates passed in jobaa3218ce. Generated builtin registries and documentation remain unchanged; all audits report zero errors. Only test-group indentation was normalized after verification, with identical non-whitespace line content.

### Verified206: retired globals and dynamic initialized reads

Program-global unset now clears the canonical symbol before releasing its previous cell, preserving escaped old references and absence even when destruction throws. The target-neutral UnsetGlobal instruction uses the shared ABI helpers and is included in eval synchronization inventory.

Dynamic and physical reflection reads preserve their original source profile in compact EIR metadata. Missing runtime names and removed untyped fields throw the shared Initialization Error; quiet probes, initialized null, typed slots and trusted reads preserve their existing contracts. Reflection metadata uses a direct profile word, retaining the 112-byte instruction budget. Interpreter reflected property reads apply the same initialization check after accessor selection.

The shared extended fixture covers root detachment, old reference readers, dynamic and reflected removed fields, compound reads, unknown runtime names and throwing global destruction. All fourteen acceptance gates passed in job6bce6292: compiler/archive build, all five emitters, twelve PHP/LFC optimizer/profile native combinations, interpreter dispatch parity with balanced owners, malformed source-profile rejection, existing scope/global/reflection/layout controls, diff and assembly hygiene, and complete builtin documentation generation/audits. Generated documentation remains unchanged. Initialization is still staged; no family is admitted by this step.

### Verified207: presence-aware literal eval scope reads

Literal eval planning retains its immutable source profile and selects materialized scope access for Initialization. The authoritative plan supplies the generated function ABI; permissive read-only fragments retain direct parameters. A typed EvalScopeContains observation reuses the scope-get ABI and visible-entry flags, preserving initialized null independently of absence. Scope-only ordinary reads and quiet probes share the existing initialization helpers. Eval synchronization includes initialized-null locals, and runtime feature discovery and named-local optimizer barriers recognize the new observation.

Five focused cases cover native null imports, conditional removal, null created by an earlier fragment, and absent/null isset, empty and coalescing. Forty PHP/LFC optimizer/profile executions report the expected values and clean heaps. All five emitters, malformed EIR result rejection, permissive planner control, parameter-shadow inventory, target mask guards and the 112-byte instruction budget pass. Eleven final gates passed across jobs68d5513b andd95b98d0, including refreshed compiler/archive, assembly hygiene and complete builtin documentation generation/audits. The second job corrected only a test fixture's overpermissive static-call predicate; production sources were unchanged. Generated builtin documentation remains unchanged.

Expanded probes also locate the next boundaries: importing a canonical absent global creates a visible native scope entry, and eval unset fragments use the interpreter fallback while Initialization is intentionally still pending. A previous strict-locals eval binding-end owner control reproduces identical five-block/176-byte retention on unchanged signed HEAD8fc582de9a in jobfe95fe0b. That is not introduced here, but remains required repair before complete strict acceptance. No additional family is admitted.

### Verified208: absent global imports and retirement ordering

Native scope-set distinguishes a null cell pointer from a nonzero PHP-null box. An absent first import creates no visible entry. Replacing an existing entry publishes zero and dirty absence before retiring the old owned cell, preserving absence through a throwing destructor. The cohesive absent_store helper handles both target ABIs within the established scope-set frames; nonzero assignment retains its existing path. No new ABI or layout is needed.

The interpreter scope-set FFI routes null cell imports through its existing unset operation. Unset marks the name absent before retiring origin metadata and storage; the shared contained release collector consumes displaced owners even when origin retirement already produced an exception. Nonzero boxed cells remain visible.

Four native cases cover first absent import, a stale prior eval snapshot, initialized null and repeated reads after a displaced destructor throws. Thirty-two PHP/LFC optimizer/profile executions pass with clean heaps, including Initialization plus Locals. All five target emitters and runtime generation, native publication ordering on every ABI, the interpreter import contract, existing scope ABI controls and the previous forty native scope-read executions pass. Eleven final gates passed in jobba397497, including the refreshed compiler/archive and staged diff/assembly hygiene. The preceding development job1ddb5b78 passed the initial four gates. No additional family is admitted.

Initialization still needs real interpreter-fallback/public acceptance, remaining property boundaries and complete source/host/cache/docs/example checks. The earlier eval binding-end owner regression remains required repair, followed by the other pending strict families and the full umbrella.

### Verified209: recreated reference-capture frame owners

Runtime-idempotent reference promotion now republishes the owning frame word before a closure retains a recreated cell. Read-only captures request that promotion even when conservative alias facts already describe the name as a reference. The shared Mixed storage helper also promotes before its early return: an initialized frame word can contain zero after a conditional detach, so initialization alone does not prove attachment. Existing source cells, owner slots and target ABI remain authoritative.

The previous five-block/176-byte eval binding-end retention is repaired. Isolated probes locate it only on the successful write after conditional unset; the rejected old-contract write and the other capture families were already clean. The first change to read-only capture selection did not cover the writable Mixed early return; the common helper correction makes both the isolated and complete regressions clean. Development jobbec73aa4 passed native and five-target checks.

Twenty isolated PHP/LFC optimizer cases now keep all other fixture declarations while selecting one invocation. Existing complete native checks, first-producer captures, parameter origins, unused captures, raw-host reference arguments, snapshot ABI checks and distinct owner-slot checks pass. A stale structural detach assertion also fails on unchanged signed HEADa667da2d33 (job286b45a3). It now verifies raw retirement, detachment, reference-owner release and typed zeroing in their current order on all targets. A new no-flag executable loop control matches PHP's shared-string updates and reports clean heaps under both optimizer settings; its initial test golden was corrected after a direct PHP cross-check.

Fifteen final acceptance gates passed across jobs56331a0b ande63484cc, including refreshed compiler/archive, eleven previous focused controls, the new executable string control and staged diff hygiene. The final continuation changed only tests; verified production sources were unchanged. Temporary EIR dump writes were removed before acceptance. No strict family is admitted by this repair. Remaining Initialization property and public/interpreter boundaries continue before the other four pending families and full umbrella acceptance.

### Verified210: initialized dynamic object storage and operand owners

Statically known stdClass receivers now share the existing presence-aware dynamic property-hash reader for ordinary initialized reads. The extracted property_hash_reads helper returns an owned boxed value and preserves null independently of hash presence; callers cast once. Named reads use the shared property-name diagnostic, while runtime names use the generic diagnostic. Quiet modes and trusted/default reads keep their selected policy. No EIR opcode, runtime ABI or storage layout was added.

Interpreter missing native dynamic names apply Initialization after metadata and getter checks. Public stdClass unset reaches the existing authorized native unset capability, whose runtime implementation already removes dynamic entries. The FakeOps adapter models that physical removal with its existing stable borrowed handles. Literal reflection is shared with native; the interpreter additionally covers runtime reflection names.

New owned-receiver probes exposed a 728-byte graph stranded when the property read threw. initialization::pin_read_operands reuses the ordinary call-owner primitive around physical initialized reads. Receiver-before-computed-name and computed-name-before-variable-receiver evaluation also use that primitive, unconditionally, because explicit expression throws can occur without strict flags. The normal path keeps its existing releases; same-frame unwinding consumes the parked owners. Tests verify the KI evaluation order and clean heaps for named/dynamic fresh receivers and a throwing name expression.

Seventeen final gates passed in jobfbaab27c: refreshed compiler/archive, all five emitters, twelve PHP/LFC optimizer/profile native executions, exact shared named Error message, owned key before missing receiver, eight interpreter profile/reflection/dispatch combinations, existing scope and extended property native/interpreter controls, target runtime property-hash/unset controls, target assembly checks, diff/assembly hygiene and builtin EIR boundary audit. The no-flag golden preserves prior read/update behavior and warnings while the shared expression-owner fix balances cleanup. Earlier development job1db00d2b passed the six direct gates. Temporary diagnostic logging was removed before acceptance.

AOT dynamic-property increment syntax and runtime ReflectionProperty constructor names remain explicit existing unsupported surfaces, tracked separately as tasksb4563345 and8fb0ab51; supported compound assignment and literal reflection forms are tested here. Runtime-name magic accessor failures were measured separately and remain active, with their source retained in task scratch. Boxed receiver paths, complete Initialization source/host/cache/public/eval/docs/example acceptance, the other four pending strict families and the full umbrella remain required. No additional family is admitted.

### Verified211: boxed initialized reads and shared dispatcher policies

Boxed and union stdClass named reads now use the checked shared property-hash reader. Initialized reads retain the full class ladder even when no declared field contributes a candidate. Runtime-name Mixed reads consume their existing saved operands through the same reader, preserving one stack retirement before an Error. Initialized null and quiet missing/null receiver probes remain distinct.

Shared property-read helpers now recognize ProfiledPropertyFetch modes, key entries by lexical scope, fetch mode and complete optional source profile, and reconstruct the original immediate. Unprofiled trusted instructions stay separate. The regression fixture repeats read and probe functions to exercise actual helper sharing, typed uninitialized probes and a name absent from the declared catalog. A focused metadata test verifies reconstruction and four distinct policy groups.

The combined Locals/interpreter fixture exposed a FakeOps class-relation bug: type_tag dereferenced persistent cells, but object_is_a used the wrapper address for object metadata. Following references before class lookup restores union parameter validation without changing compiler or runtime contracts. Temporary pending-error diagnostics were removed before acceptance.

Fourteen final gates passed in jobac872c02: refreshed compiler/archive, all-five emission, twelve native PHP/LFC optimizer/profile runs, shared-helper policy regression, four internal interpreter/profile combinations, preceding direct-object and scope native/interpreter controls, malformed profile validation, staged diff/assembly hygiene and builtin EIR boundary audit. Development job6cebba7f passed all five direct gates. The permissive stdout control also matches PHP with diagnostics directed to stderr per invocation; no PHP configuration was changed.

No new strict family is admitted. Runtime-name magic property dispatch remains the next measured Initialization gap, followed by complete source/host/cache/opaque/public/docs/example acceptance, the other four pending families and the full umbrella.

### Verified212: initialized magic property protocols

Runtime-name and boxed named reads now select removed untyped slots through the
same EIR initialized-slot/getter/probe protocol as concrete named reads. Shared
slot eligibility checks actual getter accessors and source visibility, rather
than mere presence in the property-hook metadata map. Initialized null remains
a physical read; typed fields retain their existing initialization checks.
Runtime class selection preserves inherited getters, and unknown names retain
the existing boxed magic/hash fallback. Receivers, names and results remain
rooted across getter effects and exceptions, with borrowed unboxing effects
provided by the authoritative EIR helper.

Nullsafe coalesce selects a magic accessor before a physical missing-slot guard
can divert it to null. Quiet container fetches select `__get` directly and never
interrogate `__isset`. The existing unsupported runtime-dependent magic array
property unset shape remains diagnosed on all five targets in both profiles.

Fifteen focused gates passed in jobceaed51e: compiler/eval archive build,
all-five emission, eight native PHP/LFC optimizer/Locals combinations with clean
heaps, four interpreter dispatch/Locals combinations, quiet-fetch selection and
existing shape diagnostics, preceding boxed/object/scope native and interpreter
regressions, source-profile validation, staged diff hygiene and the builtin EIR
boundary audit. The shared fixture's output matches PHP with diagnostics sent
to stderr per invocation. Temporary EIR/metadata diagnostics were removed.

No new family is admitted. Initialization source/public host/cache/opaque eval,
documentation and example acceptance remains next, followed by the four other
pending families and the complete umbrella.

### Verified213: public initialized reads and accessor guard retirement

Initialization is now independently admitted through the public checker, CLI,
interpreter API, native eval bridge and REPL. It preserves initialized null,
ordinary missing-read errors and intentional probes across native frames,
canonical globals, properties and literal/interpreted eval. Public checks cover
`strict_types=0`, PHP-only source, tagless LFC includes, explicit option override,
optimizer modes, immutable profiles and permissive/strict cache warmup. Four
families and the complete umbrella remain gated.

A removed untyped slot's getter now uses typed `InitializedGetter` method-call
metadata while retaining the shared argument, reference and owner lowering.
Validation checks its feature requirement, data identity, operand shape and
throwing/global effects. The guard key is receiver identity plus exact property
bytes; getter keys use a reserved alignment bit in the existing Fiber-local
accessor chain. The four-word runtime node and callback ABI remain unchanged.
Shared setter unwind/frame helpers were extracted rather than duplicated.
Same-pair reentry throws the initialized-property Error, while another property
or receiver can invoke its getter. Every normal and exceptional exit retires the
node, and scalar/string/Mixed returns survive the nonallocating pop. Native eval
uses the same chain; abstract providers retain distinct getter/setter identity
namespaces. Existing legacy accessor selection remains unchanged elsewhere.

The new example also exposed an existing conditional-copy leak in both modes:
a concrete local read as Mixed created a box before assignment cloned it. The
clone was retired, but the provisional source box was omitted. Assignment copies
now use the existing deferred local-view cleanup predicate. Finalization still
removes releases when final storage lends its own box. Twelve string/owned-string/
integer copy cases pass with clean heaps in permissive and strict modes, with
optimization on and off; the LFC case and runnable example are clean too.

Thirty-four focused final gates pass across the retained successful cases from
jobs804b85ab,7db7b54e,a7f1ccc1,d1d04565,e14f772f. They cover all-five public/guard
emission, malformed metadata, instruction-size budget, native and interpreter
getter protocols, cache/profile boundaries, CLI and eval global removal, real
opaque getter interop, REPL recovery, LFC/example, deferred ownership, shared
setter/Fiber controls, staged diff and assembly hygiene, and the builtin EIR
boundary audit. Failed runner registrations were not counted: the unchanged
runtime-cache identity test belongs to the CLI binary rather than the library,
and exact leaf matching disambiguated an owner test from its no-IR-opt variant.
README, CLI, eval and internal ownership/type-checker docs describe the new rule.

### Verified214: checked native dimension reads

Ordinary array, hash, boxed and string reads now carry typed CheckedElementRead
metadata. A shared effect contract and validator require Indexing, a supported
read operation and catchable allocation/unwind effects. The source feature
selector excludes trusted internal code. Presence is decided inside the existing
lookup, preserving initialized null and evaluating/normalizing each key once.
Missing elements raise OutOfBoundsException; negative in-range byte offsets,
isset, empty and coalescing probes retain their previous behavior.

Existing-element reference selection for by-reference foreach uses the same
policy, including its synthetic alias path. Slot-creating reference assignments
remain distinct. Pending operand owners reuse provisional local guards and
hidden temporary roots; final storage prunes borrowed boxed views while keeping
fresh scalar-to-Mixed boxes protected. A proven bounds optimization discharges
both the check and its metadata. Runtime mode bit two is separate from warning
and fetch-for-write bits. Both architectures emit the shared Throwable ABI.

Ten focused gates pass in job66a611da: all-five target emission/validation;
four native PHP/LFC and optimizer combinations with clean heap, source/key
order, finally, reference reads and one float-key diagnostic; metadata refusal;
permissive/internal source policy; instruction size; checked bounds proof;
boxed local guard pruning; existing temporary guard pruning; and implicit
exception registration, emission, catch and uncaught controls. Native checked
reads retain OutOfBoundsException and its ancestors through both class
registration and runtime metadata filters, even when PHP names only Throwable. Public
Indexing admission remains gated until interpreter and entry-point parity.

The final compiler/Magician archive build, diff hygiene and assembly-comment
checks pass in jobb1740f4a. No new compiler warnings were introduced; the
existing Magician fetch_update deprecation remains.

### Verified215: interpreter presence results and explicit quiet reads

RuntimeValueOps now exposes a required one-lookup checked read that returns an
owned present cell, including null, or no cell for absence. Native and fake
providers implement it. The generated C callbacks normalize a key once and use
report mode8, distinct from native checked-throw mode4 and existing warning/write
modes1/2. Real misses return a null pointer without allocating or throwing through
Rust; the interpreter creates OutOfBoundsException in its normal pending-throw
channel. Invalid-string-key quiet behavior stays separate from the new mode.

Ordinary eval array/hash/string reads consume presence results. A shared quiet
helper serves intentional coalescing/probe and writable-root/unset reads, while
key expressions retain their ordinary errors. Initialization is not implicitly
enabled. Reference argument slot selection retains its prior creation protocol;
ArrayAccess and native object dimension hooks retain their own dispatch. Native
boxed-array empty slots now honor checked absence while unchecked behavior and
present boxed null remain distinct. Internal runtime docs describe this ABI.

Ten focused gates pass: three staged interpreter read/probe/creation and owner
families in jobd3624f0b; all-five callback emission, all-five nonthrowing report
paths, native source/optimizer/owner controls and two existing string/write-fetch
mode regressions in job70bceff0; independent exact-key/initialization policy pairs
and the existing initialized-frame owner fixture in jobfaa7abda. ArrayGet and
array construction are still bytecode CompileGap cases, so tests explicitly
verify dispatcher fallback to the walker instead of claiming a new VM opcode.
Public Indexing admission remains gated pending native opaque eval, public CLI,
REPL, source/cache/boundary checks and its example.

Default compiler/Magician archive build, diff hygiene and assembly-comment
checks pass in jobb3f7bebb. The existing warning baseline is unchanged.

### Verified216: public checked indexing admission

Indexing is independently admitted through the public checker, CLI, interpreter
API, opaque native eval and REPL. Thirteen families are integrated; Nullability,
Mixed, ArrayTypes and the complete umbrella remain gated. A CLI smoke test can
explicitly disable those three pending families while retaining the other thirteen.
The negative Indexing override is tested with an ordinary missing read that
returns to its permissive warning path, rather than a probe valid in both modes.

Public fixtures cover PHP-only source, both optimizer modes, tagless LFC includes,
literal missing reads, initialized null, negative string offsets, missing-read
statements, source/key effects, one float-key diagnostic, finally and owner
cleanup. Native reference element selection, supported slot creation, removal,
coalescing assignment and read-modify-write cases are distinguished. The eval
fixture uses supported by-value foreach and explicit writes, since reference
foreach is an existing parser boundary; its profile-independent refusal is pinned.
Object hooks are authoritative: direct and boxed ArrayAccess reads invoke one
getter and no extra offsetExists, and nullsafe probes stay quiet in AOT and eval.
The new stock example distinguishes initialized null, missing items and probes.

The C reader callback retains the established alias-key planning and conversion.
A canonical-tuple bit16 now tells the shared receiver reader not to normalize
that completed tuple again. This avoids duplicate work without the double float
deprecation produced by a discarded original-boxed-key transport experiment.
Existing warning/write/throw/report bits1/2/4/8 remain independent. Both permissive
and exact-key real native bridge regression families pass. Runtime assembly also
passes clang for every supported target, including both iOS variants.

Public API warmups explicitly install a bytecode cache key and verify the cached
array fallback across permissive/strict/permissive profiles, with clean owners.
The runtime-cache identity gate runs from its actual binary harness, not --lib.
All-five public checker/emitter checks, native CLI/eval/REPL/source tests and
legacy bridge controls are recorded in the step216 evidence ledger. The existing
escaping eval parameter/global alias value issue was measured without strict too
and recorded separately as taskc1397da5 (knowledgecfa9012a); no new alias lifetime
behavior is claimed here. Public and internal docs match the admitted boundary.

Seventeen distinct focused feature/boundary gates pass. The final compiler and
Magician archive build, diff hygiene and assembly-comment checks pass in
job9511b229. Existing warnings are unchanged; no broad local suite was run.
