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

Verified through step123 (this commit), with remaining feature integration in progress. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask contains Locals, Bools, DynamicCode, Opcache, Arithmetic,
Overflow, Strings, Coercions, Casts and Comparisons. Locals includes the verified116-120 numeric/value binding, native reference and eval extensions;
its complete reference/global/eval and broader binding-flow contract remains outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

This progress ledger condenses verified implementation through step120.
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
still needs numeric, reference, global and flow coverage. Nullability/Mixed/Initialization, exact keys/
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
| 120 | this commit | feat(strict): preserve scalar and nominal local value contracts |


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


### Verified115: native eval binding presence

The native snapshot publisher now lives in
src/codegen/lower_inst/builtins/eval/native_local_contracts.rs. An Object slot
with a zero occupant clears its snapshot contract instead of publishing a
future inferred class. Existing object occupants keep their declared native
contract. The shared target-aware load and branch helpers implement this on all
five targets without a new runtime symbol or ABI. Non-object behavior is unchanged.

Job487778c9 passed the compiler build and four exact gates: the all-target
presence emitter, the original catch-scope TVN regression in both IR modes,
existing typed native alias rejection/unset behavior, and Magician's pre-store
object snapshot validator. The builtin EIR architecture audit also passed. Total duration was 5m23s;
the catch-scope regression took 71.73s and existing typed native alias coverage
took 86.79s. Heap accounting is clean in both optimizer modes. The task-owned
verification directory is removed after evidence capture. This is preparatory
correctness work for stronger Locals, not its admission.


### Verified116: numeric local binding contracts

Numeric local contracts are separate from read narrowing and physical storage in
src/types/checker/numeric_local_contracts.rs. LocalStoreContract retains the
owning feature; neutral/compiler membership plans and rooted EIR guards preserve
Locals independence from Coercions and Overflow. Statement/expression writes and
prefix/postfix updates validate before mutation. AST effects and exact TypeError
summaries retain their handlers. Wrong numeric kinds produce source diagnostics
when known, or TypeError on a dynamic write; the previous value remains intact.

A closure's cloned enclosing environment is not binding evidence. Fresh locals
use actual scope/binding facts, replaced reference targets retire their old
inferred contract, self-alias preserves it, and diagnostic-only inference restores
numeric origins. Proven bound non-null variables and intrinsic non-null values
have nonexecuting coalesce defaults; absence-aware property/offset probes retain
their real default effects. Explicit mixed/numeric unions and legitimate unset/
rebind retain their contracts. Source/type origin still requires the broader
reference/global/eval and scalar-flow audit described above.

Job5705df4f passed the compiler build, twelve exact gates and builtin architecture
audit in 2m52s. Both native IR modes have clean heap accounting. The expanded
emitter/library gate covers all five targets; native execution is Linux x86_64.
The [binding investigation history](strict-semantic-profile-bindings.md) retains
intermediate failures, controls and the remaining source-point design notes.
The owned verification directory is removed after evidence capture. Continue the
full task: this is a numeric binding step, not completed stronger Locals or the
full umbrella.


### Verified117: nullable numeric bindings and shared updates

Numeric contracts retain explicit null alternatives, and successful guards keep
the narrower kinds actually produced by an expression. The existing completed-
store proof preserves a non-null read after an accepted compound arithmetic
result, so a valid int return is not weakened to int|null. Nullable reference
updates validate an ordinary PHP-promoting candidate before the raw tagged
storage changes. With Locals alone a float result raises TypeError; independently
enabled Overflow raises ArithmeticError first. Reference layout eligibility and
permissive storage behavior are unchanged.

Job8d7c8fcc passed the compiler build without new warnings, ten exact gates and
the builtin architecture audit in2m39s. Both IR modes and Overflow settings have
clean heaps; MIN/MAX, shared updates, repeated prefix/postfix results, explicit
null, exact large integers, typed compound returns, the example and existing
nullable reference/coalesce regressions pass. Both nullable and ordinary EIR/
library emitter gates cover all five targets. The
[binding history](strict-semantic-profile-bindings.md) retains baseline and
intermediate diagnosis.

Continue contract transport across untyped reference parameters (a7dce035) and
opaque eval (a6343333). Direct local aliases/global writes and fresh integer
closure shadowing are passing signed116 controls. Exact future regressions are
preserved, not yet registered, in tests/fixtures/strict/native_untyped_local_contract.php
and native_eval_local_contract.php, excluded from117 staging. The owned baseline/
verification directory is removed after evidence capture. Complete broader Locals,
the other six families and full umbrella/host acceptance before ending the task.


### Verified118: caller contracts through native references

Numeric binding contracts now survive boxing for untyped/mixed reference
parameters and runtime-selected native descriptors. Checker metadata records the
semantic contract even when a declared numeric union already uses Mixed storage.
BindLocalRefContract explicitly carries the declaration through EIR, including
textual output and profile validation. A shared C-ABI callback factory supplies
borrowed original inputs and independently owned accepted candidates. Exact
numeric membership callbacks use the existing protected reference coordinator,
canonical identity and weak source registry. Hidden owner bindings unregister
before unset, normal frame cleanup and exceptional cleanup. Physical reference
eligibility and permissive behavior remain unchanged.

Jobbd8c3124 passed the compiler build, eight exact gates and builtin architecture
audit in2m46s. Both IR modes have clean heaps for invalid and valid numeric
references, exact large integers, parameter entry, nullable writes, explicit
numeric union rejection, explicit mixed openness, runtime-selected descriptors,
unset and exceptional frame exit. The extended example and existing ordinary/
nullable numeric guards pass. EIR/native callback generation and existing property
metadata cover all five supported targets. Native execution is Linux x86_64.

This resolves the recorded native untyped-reference baselinea7dce035 for numeric
contracts. Intermediate jobs8e574690,18ed5c3f,4b25fd6f,7f65ab1a and24539791
exposed and isolated unknown-descriptor and already-boxed union gaps before final
acceptance. Their verified successors are permanent regression fixtures. The
owned verification directory is removed after evidence capture.

Continue native/eval source-point contracts (a6343333), broader stable bindings,
the remaining six feature families and complete umbrella/host acceptance. The
saved native_eval_local_contract.php fixture remains untracked for that work.


### Verified119: native numeric bindings during opaque eval

The checker records initialized source-point numeric origins at each eval site.
Before an interpreter barrier, EIR promotes only eligible current owned bindings
through the existing canonical reference snapshot protocol. The shared protected
reference publisher therefore validates an interpreted replacement before the
native child changes. This reuses118 callbacks, source registry and snapshot
ownership without adding an eval ABI or reconstructing a contract from a future
frame slot. Eval unset/rebind detachment unregisters the weak source before
retiring its native owner. Existing raw reference layout eligibility is retained.

Jobf0dec17e passed the build, five exact gates and the architecture audit in6m04s.
The two target gates check existing binding origins and native validator/emitter
contracts on all five targets. Both optimizer modes and default/Locals profiles
run with clean heaps, covering rejected float writes, null, exact large integers,
explicit numeric unions, explicit mixed, unset, and future scalar slots. Existing
reference lifetimes and the future-catch-scope regression pass. Probe310ef94b
independently showed the original native/eval baseline now retains integer1 with
a catchable TypeError. An earlier bare-library eval test lacked the production
object-cast prelude for the full opaque catalog; it was replaced by focused
source-origin checks paired with real CLI/native execution.

Final job2861d910 passed the build and two exact gates in2m30s after naming the
long native test as a bundle and tightening the future-slot fixture: its later
integer assignment is on a nonexecuting branch, so the fixture tests absence of
an earlier contract without requiring an incompatible later binding write. The
opaque eval bundle took114.74s and has the established360s CI budget. Both
nextest profile overrides and assembly-comment alignment were checked.

This resolves the original numeric native/eval baselinea6343333. Remaining
Locals work includes nonnumeric and runtime-established binding origins, reverse
eval-to-native transport, globals/captures/foreach and array-kind independence.
Probec045a07a confirms dynamic Mixed replacements still bypass nonnumeric
inferred string, boolean, null, object and array-kind contracts; explicit mixed
and replacing array elements remain passing openness controls. Complete those
boundaries, six unintegrated feature families and full umbrella/host acceptance.
The owned target/strict-native-eval-8r76iwqo directory is removed after baseline
evidence capture; follow-up fixture sources are preserved when needed.


### Verified120: scalar, nominal and array-kind local contracts

The shared origin helper is now src/types/checker/local_binding_contracts.rs;
inferred_local_contracts replaces the numeric-only per-body map. Scalar/null,
nominal and union membership uses the existing independently owned Locals plans,
rooted pre-store checks and protected native reference callbacks. Inferred array
contracts preserve the PHP array kind without pinning element or packed/hash
storage details; a narrow inferred-array merge accepts that guarded broad view.
Concrete declared element contracts retain the existing structural/layout checker.
Callbacks now cover scalar, false/null, class and array membership. Accepted
boxed nulls become unboxed null storage after retiring only their owned source,
fixing the40-byte leak exposed by a valid null replacement.

Job8f868713 passed build, five exact gates and architecture audit in3m42s.
Probe7bd2efe1 validated scalar, subclass, array, false/null, union and declared
array controls, and exposed the boxed-null leak before acceptance. Job21481870
then passed build, eight exact gates and architecture audit in6m02s. The expanded
all-five-target gate includes nominal/array/false/null callbacks. Native both IR
modes have clean heaps for rejected and accepted writes, explicit mixed/union,
array element openness, nullable numerics, unset/rebind, opaque eval and future
catch names. The opaque eval and catch regressions took115.88s and57.52s.

Final job58a74c9d passed build and six exact gates in42s after adding string/null
references, one-time destructor retirement on rejection, the extended example
and existing include/CLI flag controls. Both native modes are heap-clean;
legacy literal-eval freedom still passes its old test and is identified below
as an unfinished stronger-Locals boundary, not accepted final semantics.

This resolves direct/ordinary-reference nonnumeric baselined59f314d. Remaining
stronger Locals includes runtime-established/reverse eval origins (b99d1b21),
known literal eval decision maps, and mutating builtin publications. Probeb863be83
shows settype still changes bool to int and int to string under Locals; the latter
also leaks an original40-byte int box in both profiles. Numeric string increment
is a passing control (Locals rejects the promoted int and preserves string9).
Null array append remains an inherited backend refusal in both profiles.

The full approved task remains active: complete these boundaries, broader
global/capture/foreach flow and remaining feature families, then verify the full
umbrella and hosts. The task-owned verification directory is removed after
baseline evidence capture; the reverse-eval and settype sources remain saved for
the next implementation steps.


### Verified121: mutating conversion and typed reference source contracts

Locals-owned settype calls use the existing checked conversion/publication graph,
including literal targets, runtime targets and prepared callable bodies. Literal
success-only type updates no longer poison a catch's original view. Each candidate
passes the shared local membership boundary before publication, while explicit
mixed remains open. Locals selects reference/copy protocols independently of the
Casts and Coercions policies. Prepared aliases bind the original caller source
before introducing their synthetic names; call_user_func values retain separate
argument cells.

A shared checker helper records caller source contracts for resolved, inferred
and descriptor function calls. EIR prepares eligible owned bindings before typed
reference arguments, so a later widened frame layout cannot expose a boxed pointer
as a raw bool payload. This preserves named-argument source order, rejected writes
and the current false value without leaking the original boolean box.
Permissive scalar specialization retains its raw ABI; boxed legacy settype output
now uses the shared owned-output publisher and retires its previous occupant.

Job00d2da5e passed the all-five-target source-registration/emission gate. Job53a27848
passed the build and six exact gates: new target/native settype cases, extended
example, existing checked callable values/owners, permissive prepared bodies and
valid local reference/union membership. Both native optimizer modes have clean
heaps for literal/runtime targets, first-class/runtime-selected callbacks, value
copies, named source-order effects, null, explicit mixed and array membership.
Permissive bool-to-int and argc-to-string controls preserve values and clean heaps.
Final jobee32fb15 passed the full builtin documentation generation, site compatibility
and architecture audits, restored default Magician/compiler archives and passed
two exact numeric reference lifetime/nullable update controls in14s. Generated
registry/pages have no diff. Assembly comments and git diff hygiene pass.

The complete implementation remains active: literal eval decision maps and their
old permissive expectations (37ae1f9b), runtime-established reverse eval origins
(b99d1b21), remaining global/capture/foreach/mutation flow, the six unintegrated
feature families and full umbrella/host acceptance still require completion.


### Verified122: imported native contracts across literal eval

Literal scope writes prepare existing caller source contracts even when the AOT
planner avoids an interpreter barrier. BindLocalRefContract now selects typed
canonical promotion for a supported owned payload, without relabeling the raw
alias ABI. A null payload has an explicit canonical layout, and Locals-only null
snapshot and reload support keeps raw null references visible at this boundary.

A scope-set input flag distinguishes assignment from native snapshot installation.
Both Magician scope assignment and the scope-only native map use the existing
protected reference_replace_v2 and release_v3 contracts. The native helper uses
shared ABI operations, preserving its old wrapper while validating before mutation
and consuming candidate/displaced owners before propagating a contained Throwable.
EvalScopeSet now explicitly models MAY_THROW. Repeated installation of an owned
snapshot retires the previous lease even if both pointers are equal, fixing the
pure-scope wrapper/child80-byte leak found during acceptance.

The native scope-only implementation and Magician scope implementation are distinct.
A literal unset in a combined fixture selects Magician for the module, so it cannot
stand in for scope-only coverage. Isolated scalar probes first exposed that missing
native path, which is now independently tested. The pure target gate asserts that
Locals plus DynamicCode remains AOT and does not select the interpreter bridge.

Joba09659ce passed the build and eight exact gates before the updated example
exposed the existing body-wide eval/unset restriction. Those gates cover pure and
bridge native scalar/null/reference/union/mixed behavior in both IR modes with
clean heaps, all-five-target EIR, legacy imported-binding expectations, numeric and
settype controls, layout classification, and real clang assembly for scope helpers
and canonical promotions on all five targets. The example's eval demonstration is
in a dedicated function so the earlier unset demonstration keeps its own body.
Final jobe23097c1 passed the build, updated example and opaque-eval bundle in2m08s;
the opaque bundle took116.72s. All changed emitter comments and diff hygiene pass.
Default Magician/compiler archives were built. No full suite, push or PR was run.

The full approved implementation remains active. Fragment-owned local decisions
still need isolated source facts or runtime origin transport: the first part of
test_eval_fragment_composes_with_strict_locals deliberately retains its previous
own-local expectation for now; its imported native-binding part now catches the
TypeError and observes integer1. Reverse eval origins b99d1b21, broader binding
flow and the six remaining feature families plus umbrella/host acceptance remain.


### Verified123: eval finalizer value ownership

emit_eval_scope_finalizer now roots the loaded publication value across a throwing
EvalScopeSet, unpins on normal return and retires that loaded value after the scope
has taken its independent owner. The former finalizer emitted a MaybeOwned string
load and scope store without releasing its value copy, leaking one string per
activation. The same helper covers normal and rejected publication without a new
target-specific path or changes to binding acceptance.

Job3f893fed passed build and two exact native gates in1m26s. Final job762d9fc5
passed build, three exact gates and diff hygiene in1m54s after sharing the valid
same-kind string fixture with the all-five-target literal emission gate. Default
and Locals, both IR modes, repeated fresh/open scope activations and rejected
publication have clean heaps. Existing pure/bridge literal binding behavior passes.
The regression uses same-kind string stores, so it does not encode the unfinished
fresh-binding retype freedom as an approved requirement.

Origin baseline45552126 remains separate: the identical fragment assigning int
then string is shared by a fresh caller and an explicit-mixed caller, and both
currently print changedW|changedO|. Only the fresh activation should reject.
Reverse opaque-eval origin still prints Winteger|1 in both profiles. Continue the
full approved implementation with source-origin transport and all remaining
profile families, preserving legitimate mixed/union producer openness.


### Verified124: source-ordered literal eval publication

With Locals enabled, literal fragments with writes select the existing direct
scope publication mode. Every assignment reaches the protected source contract
before later PHP effects, even when a subsequent assignment would overwrite it.
A shared rooted publisher protects in-flight values for both direct stores and
finalizers. Finalizers retain their explicit loaded-value release. Explicit mixed
imports stay open and inferred PHP arrays retain their kind without silently
enabling ArrayTypes. Earlier accepted writes survive a later rejection.

Job6e007a99 passed the build and five exact gates in39s: all-five-target EIR
emission, new source-order regression, finalizer ownership, imported native
contracts and the updated example. Native regression covers pure and bridge
scopes, both IR modes, owned producer effects and rejected array-kind changes,
with clean heaps. Diff hygiene passes. Full implementation remains active: fresh
and runtime-established eval origins, remaining binding flow, six pending feature
families and complete umbrella/host acceptance still need implementation.


### Verified125: activation-owned literal eval origins

The dependency-neutral reference bridge now keeps scope/name origins in stable
two-word slots. An imported or explicitly mixed producer records an open origin,
which later concrete writes cannot close. Fresh literal producers select an exact
local validator and one canonical managed reference cell. Identical compiled
fragment bodies therefore retain different contracts in separate activations.
Unset and scope retirement detach native source records before their cell owners
retire. Native teardown contains PHP exits before returning to Rust. Both native
linked-list scopes and Magician scopes use the same origin publisher and lifetime
helpers. Exact PHP array kind contracts remain independent from ArrayTypes.

EvalScopeStoreImmediate explicitly carries the static name and producer contract.
Its entire record is boxed to preserve the production112-byte Instruction bound;
boxing only the type still enlarged the immediate payload. The EIR validator
rejects noncanonical contracts and missing Locals profiles. Callback generation
reuses the existing local membership and protected reference coordinator.

Registry job08b938cd passed two exact stable-address/open-origin/retirement gates.
Job94229e53 passed default build and pure plus bridge probes, including split
fragments and repeated unset/rebinding, with clean heaps. An earlier probe found
a C-ABI cell argument incorrectly passed in the result register, corrected before
acceptance. Jobd4e5609e passed build and eight exact gates in4m20s: five-target
EIR, real five-target helper assembly, fresh/imported/mixed/null/array native
origins in both IR modes, legacy composition, source ordering, finalizer owners,
imported native contracts and example. Jobbfeb6b59 passed the full builtin docs
workflow, all audits and default archive restoration in1m58s, with no generated
diff. The final scope-core assembler gate exposed an Apple conditional branch to
an external label. It now branches locally before an unconditional propagation
branch. Finale5c2550f passed build and four exact five-target/native gates in2m34s.
Assembly comments and diff hygiene pass. No full suite, push or PR was run.

The complete approved implementation remains active. Native-frame exposure of a
fresh eval name still bypasses its origin: probeeb10dc54 prints
integer:Wstring:bad|Winteger:1| with clean heaps. The synchronization inventory
currently publishes every matching frame slot without an initialized-binding
guard. Next complete initialized origin transport and native-store enforcement,
then opaque-eval origins b99d1b21, remaining binding flow and the six pending
feature families plus umbrella and host acceptance.


### Verified126: initialized native bindings and suspended scope ownership

Source initialization and binding retirement now emit SetLocalBindingState EIR
events in user frames with eval under Locals. Representation-only stores suppress
these events. Independent frame presence words start unbound, including for null
slots with no payload bytes. Eval snapshotting skips unbound slots; successful
reloads and missing entries update presence independently from projected values.
A native observation no longer imports a future zero-filled slot before a literal
fragment creates its binding. Conditional mixed declarations preserve openness
only on the branch that actually executes the declaration. This does not admit
the still-pending Initialization feature.

Build493a0401 passed in1m36s. Probe86613921 passed both IR modes with clean heaps:
integer:Tinteger:1|Winteger:1|, resolving the native-observer part of eb10dc54 while
leaving the native-writer part explicit. Job1a91d15a passed build and six exact
frame/five-target/native gates in4m11s, including conditional declarations, existing
fresh origins, native imports and example.

An additional generator/GC probe1de64a1e found the inherited missing native-core
__elephc_eval_scope_visit_owners_v1 symbol even in permissive mode. The compact
linked-list scope now implements that pure slot visitor through shared target ABI
helpers. Both scope implementations also enumerate the independent managed cell
owners held by the native scope-origin registry. Enumeration preserves exact
owner-slot identity and borrows immutable registry state only across pure collector
callbacks, with no PHP execution, ownership changes or scope mutation.
Job403cd725 passed compiler/default native archives and pure suspended scope
acceptance in1m43s: default1W and Locals1T, both clean heaps. Final job03999afe
passed build and seven exact registry/EIR/real-assembler/native gates in3m55s.
All five supported targets emit and assemble the new visitor; pure and bridge
scopes, both IR modes, generator suspension plus cycle collection, native
observations and imported scalar aliases pass with clean heaps. Assembly comments
and diff hygiene pass. No builtin home/signature changed; prior generated docs
remain unchanged. No full suite, push or PR was run.

Continue the full approved task. Native stores after a literal-created float still
print Winteger:1 (tests/fixtures/strict/literal_eval_native_store_origin.php).
Opaque runtime origins b99d1b21 also remain open. Next validate original native
store candidates against surviving scope origins and preserve their physical
representation, then complete remaining binding flow, six feature families and
umbrella/host acceptance.


### Verified127: native stores preserve literal eval origins

Native assignments and typed reference arguments prepare canonical managed-cell
storage before replacing an eval-visible value. The shared preparation keeps the
scope's surviving semantic source without inventing an incoming native contract.
Static membership guards run first when present. Whole-frame Mixed storage keeps
a float value intact while the existing protected reference coordinator validates
the original candidate. Valid stores remain visible to later fragments; explicit
mixed stays open, value copies stay independent and array-kind rules remain
independent from element contracts.

Probe739e7b89 exposed a raw float-reference ABI mismatch after eval reload. GDB
confirmed float4.5 bits reached reference inspection. Preparing the source before
typed reference argument lowering fixes that boundary. Jobead4260c passed build
and complete pure/bridge probes with clean heaps. Final captured job843e0e5c
passed build and seven exact gates in1m10s: all-five-target EIR/emission, native
stores, native observation, imported native contracts, settype mutation, suspended
generator ownership and example. Native cases cover both IR modes, owned rejected
string candidates, accepted floats, typed float references, array replacement and
permissive controls. Diff hygiene passes. No full suite, push or PR was run.

Continue the complete approved implementation. Opaque runtime-origin transport
b99d1b21 remains open; then complete remaining binding flow, six pending feature
families and complete umbrella/host acceptance.


### Verified128: runtime-established eval binding origins

A focused source-aware store helper captures producer contracts before RHS effects
and records only successful fresh bindings. Inferred origins live separately from
explicit declarations and physical native snapshot contracts. Existing mixed imports
and explicitly mixed producers stay open. Closed origins reuse persistent reference
storage and weak context allocation lifetimes, so native stores consult the same
canonical reference coordinator after eval returns. Unset and scope drain retire
inferred name metadata; surviving aliases retain their shared storage constraints.

The bytecode VM has an explicit PrepareStore opcode and immutable source metadata.
Nested assignments preserve producer evaluation order without storing runtime owners
in cached code. Walker and VM consume the same helper. The existing protected exact
eval prepass now checks local-reference types under Locals independently from the
property Coercions policy; weak property coercion remains selected independently.

Build5e678b2d and basic native probe8e73ce38 passed, resolving the original
b99d1b21 baseline: default Winteger|1 versus Locals Tdouble|2.5, both heap-clean.
Job243e5fad passed build and eight exact gates in4m51s: both-engine first-value
and openness/independent-copy ownership tests, five-target prepass selection, weak
property independence, real five-target clang assembly, native opaque origins,
existing literal native stores and fresh literal origins. The native opaque test
took116.68s, covers both IR modes/default/Locals and has clean heaps for string,
array, null, reference alias, unset and mixed cases. It now uses the existing bundle
timeout suffix. Finald707cd44 passed build, both exact engine gates, unique bundle
registration, updated runtime-source example, assembly comments and diff hygiene
in1m10s. The redundant generic membership adapter was removed because the existing
validator already checks exact kinds. No builtin homes/signatures, generated docs,
full suite, push or PR were changed.

Producer baselinec737d63d is now explicit for the next step: a runtime conditional
int/float producer followed by a float store rejects, while a copied native int
producer can retype to string. It prints T|W| with a clean heap; expected is U|T|.
The current helper still needs complete alternative propagation and native logical
producer metadata, separate from physical storage. Continue this and the complete
approved task, including remaining binding flow, six pending families and umbrella
plus host acceptance.
