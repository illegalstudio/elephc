# Strict semantic profiles

- [x] Inspect CLI, source profiles, checking, folding, EIR, runtime, eval, and caches.
- [x] Record the agreed umbrella, PHP-only naming, and exact string-key rules.
- [x] Adopt the detailed boundary defaults from the approved implementation plan.
- [x] Add the shared semantic contract and resolve CLI requests once.
- [x] Rename the existing PHP-only option and its maintained integration surfaces.
- [ ] Propagate the immutable profile through compiler phases and runtime contexts.
- [x] Complete strict-dynamic-code source, execution, and REPL boundaries.
- [x] Implement numeric rules, checked conversions, and catchable integer overflow.
- [ ] Implement stable bindings, boolean conditions, comparisons, and string rules.
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

Verified through step138 (this commit). Remaining feature integration is active. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask contains Locals, Bools, DynamicCode, Opcache, Arithmetic,
Overflow, Strings, Coercions, Casts and Comparisons. Locals includes numeric, scalar, nominal, array-kind, callable/iterable binding,
resource, native reference and eval origin extensions through134;
its complete reference/global/eval and broader binding-flow contract remains outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

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

Verified114 admits Comparisons as the tenth feature, following Strings109,
Coercions110 and Casts111. Signed112 and113 provide comparator result ownership,
native source policy and optimizer retention. Public native/opaque domains,
callable/Reflection hosts, REPL recovery, example, independent flags and all-five-
target library emission are verified. Job8f891928 passed nine exact primary gates,
all builtin generation/architecture audits and restored default builds (6m43s).
Job8cf9054e passed six method-focused gates, repeated the audits and restored
builds (3m20s). Integer and heap-string argument owners retire on free-function,
instance-method and static-method failures; source arity and receiver destruction
remain correct. No generated documentation changed. Local native execution is
Linux x86_64, with emitter/library checks for the complete supported matrix.

The complete umbrella, stronger Locals and six remaining feature integrations
remain unfinished. No push, PR or full local suite has been requested or run.

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
