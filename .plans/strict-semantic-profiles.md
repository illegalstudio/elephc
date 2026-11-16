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

Verified through step110 (this commit), with remaining feature admission audits in progress. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask still contains only
Locals, Bools, DynamicCode, Opcache, Arithmetic, Overflow, Strings and Coercions. Existing Locals behavior remains available
but its stronger numeric/reference/eval binding contract is outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

This progress ledger condenses verified implementation through step110.
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

1. Audit the remaining numeric call/host paths. Verify initial folding cannot erase required
   numeric guards or typed source violations. Close any concrete remaining literal,
   compound-store, update, callable, reference and exception ownership gaps.
2. Integrate shared conversion plans for implicit assignments, parameters, returns,
   builtin inputs, callbacks, descriptors, reflection and eval re-entry. Complete
   source-sensitive/direct builtin handlers and pre-fold AOT builtin checks before
   admitting Coercions. Complete checked explicit
   casts with range/domain errors, exact strings and native object cast contracts.
   Original GMP and SimpleXML cast projections are now checked. Before Casts
   admission, finish host callbacks and the
   temporary Reflection Stringable lifetime audit. Runtime gmp_intval range guards and broad
   object/interface consumers now share reviewed checked paths. Internal
   implementation helpers must not bypass public contracts.
3. Finish stable Locals bindings across promotion, parameters, captures, globals,
   by-reference writes, foreach aliases, loop joins, unset/rebind and eval scopes.
   Locals alone must not silently enable Overflow or widen an inferred int binding.
4. Complete Comparisons, Strings, Nullability, Mixed and Initialization through
   source validation, shared checking/plans, EIR/runtime guards, builtin contracts,
   walker/VM, references and host boundaries. Preserve explicit formatting and
   intentional probing/narrowing. Follow the approved boundary defaults above.
5. Complete exact runtime keys for reads/writes/probes/unset/append/spreads and
   every producer, including JSON, casts and serialization; pin array element/key
   contracts and mutators; implement checked ordinary indexing while probes retain
   missing-value semantics. Preserve COW and references through every failure.
6. Audit executable/web/REPL/staticlib/cdylib/phpext/eval/callback boundaries and
   effective cache identity for the whole profile. Update user docs, README,
   examples and generated metadata. Admit every feature only after its complete
   guarantee is enforced, then enable the full umbrella and verify combinations.

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
- Scalar comparison, source/builtin comparison modes, bytewise strings, switch
  matching and operand snapshots have verified cores. Native value-class and
  remaining host admission checks are still required for Comparisons.
- Strings is admitted after source/native/eval/Reflection-callable-independent
  formatting checks. Native XML writes inspect the original selection before
  mutation. Logical string null survives boxing and predicates. Native activation
  snapshots share managed reference identity and retire copied leases after
  writeback; global callback scope stays materialized.

Coercions is admitted after the verified110 audit below. For Casts, retain
these explicit open checks from the historical notes: PHP's temporary Reflection
Stringable destruction order (knowledgec1351278), pre-existing references versus
references introduced by destructors, runtime/opaque ReflectionClass setters,
and Casts-only eval argument aliases. Stronger Locals work must also revisit the
native/eval catch-name collision (knowledgea7883b31), in addition to the numeric,
reference, global and flow cases in the completion checklist. Remaining
nullability/mixed/initialization, exact keys/types/indexing, hosts and cache
acceptance requirements above are unchanged.

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


### Latest verified admission checkpoint

109 (`3bc3f47cf4`) admits Strings. Native/eval alias visibility, destructor timing,
dynamic owner retention, REPL recovery, five-target guards and compiler build
passed. The full stdout/heap evidence and intermediate red cases are preserved in
the [admission history](strict-semantic-profile-admissions.md#real-opaque-eval-string-host-parity-109-verified).

### Native Coercions contract admission (110, verified)

109 is signed as3bc3f47cf4 and Strings is publicly admitted. The two preserved
eval-owned property native-reference fixtures are now registered. They require
exact Float contracts to reject Int writes at the native assignment before later
callee effects, plus native catches, nested eval, source intersections, source
removal, static properties and orphan lifetime controls. Coercions admission is
provisional only in this uncommitted audit snapshot. The real Magician archive
will be rebuilt before the two exact native tests; no execution result exists
yet. Remaining source/prelude/callable/host coverage must be checked before any
Coercions admission commit.


Native110 job116fb542 exited0 after2m52s. Assignment-point63.71s and reference
lifecycle61.03s both pass with both optimizers and clean heaps: native writes
reject before following callee effects, native catches/nested eval preserve the
original value, and removing source declarations restores permitted kinds.
Shared native parameter/return/builtin PHP fixtures pass public --check and are
now reused by staged tests plus newly registered public CLI execution checks.
A REPL exact-binding recovery case is registered. These public execution/source
and REPL tests remain unverified; Coercions admission is still provisional.


Public110 job6e2399ae exited0 after1m52s. Native shared parameter/return/builtin
CLI matrix6.40s, source evidence/explicit conversion0.59s, REPL exact binding and
recovery48.69s and compiler build39.44s pass. Before final admission, added direct
AOT Reflection controls and one real opaque host matrix spanning eval/native
parameters, returns, methods, descriptors, builtin inputs and Reflection. A
public library all5-target profile test and the extended existing strict example
complete the admission checks. These final additions are not yet executed.


4e82b956 passed all3 selected five-target compiler tests, then failed the new
Reflection fixture. Exact retry9c5a39af proves this is permissive no-IR-opt too:
expected stdout matches, but inline reflectors leak96 blocks8672bytes. Known
invoke fast paths bypass receiver retirement after lower_member_receiver already
constructed it. Both fast paths now root the original receiver and retire it
afterward through a shared helper that detaches the receiver record, pins the
completed result and clears/releases the receiver slot in LIFO order. The native
control adds an owned string result. Added CLI assembly-only checks on all5
targets, reusing a compile-only fixture helper. A separate typed-string missing
value Coercions probe is registered and unexecuted; no logical-kind claim yet.
Opaque hosts and the example were not reached by4e82b956. No110 commit/admission.


Nativec76ea9eb passed Reflection receiver/result owners13.59s (all4 profile/IR
controls), all5-target CLI Reflection emission12.38s, opaque native/eval host
contracts63.72s (both IR modes) and the combined-profile example2.39s. The new
missing-string probe then reproduced three exact-binding bypasses: argument,
return and builtin accepted logical null behind Str storage, yielding
V|NULL|0|NV:0:1 instead of T|T|T|NV:0:1, clean heap. Compiler conversion planning
now keeps runtime kind validation for accepted Str storage when the declaration
excludes null. Nullable declarations and explicit conversions retain their
contracts. The public all5-target compiler fixture now checks that predicate too.
These latest changes await focused verification; no110 admission commit yet.


Final110 verification90c1f7b5 exited0 after3m00s. Public library guards pass
all5 targets, including logical string-kind checks; the missing-string regression
passes all4 AST/EIR modes in1.56s with correct nullable/cast controls and clean
heaps. Shared public native contracts6.25s, Reflection profile/optimizer controls
14.58s and the extended combined-profile example2.03s pass, followed by compiler
build59.95s. Earlier verified opaque hosts and five-target Reflection emission
remain green; the last change is the shared concrete-string declaration guard.
Source, target, owner, REPL, reference, Reflection and explicit-conversion
evidence now supports public Coercions independently. No other guarantee was
enabled. No stored EvalIR shape or profile bit numbering changed. Known
Reflection receivers now retire on normal and exceptional paths through the
shared result-preserving owner helper. CLI/checker docs describe the guarantee.

The detailed verified notes through109 moved verbatim into the linked admission
history, keeping all approved requirements, active audits and the full ledger in
this plan while restoring room under Code Journal's100k limit. Next111 closes
the remaining Casts Reflection identity/temporary and opaque-host audits, then
stronger Locals and the remaining approved guarantees. The full user task stays
active; the umbrella remains gated until every guarantee is implemented.


### Casts Reflection binding identity audit (111, pending)

110 is signed asba2bc9f312. New PHP reference fixture distinguishes four cases:
an existing property reference versus one introduced by the temporary Stringable
destructor, each with/without a destructor throw. PHP always converts before the
destructor; without an original reference it publishes into the property and
breaks the newly introduced alias (old:second:new), whereas an original reference
receives the converted value even after property rebinding (new:second:second).
Successful conversion publishes despite a retirement ValueError. Both native
lowering and actual opaque Reflection over native declarations are prepared for
comparison. Casts admission is provisional in this uncommitted experiment only;
no native execution result exists yet. Remaining source/opaque/alias/host Casts
checks remain required before admission.


111 initial8d8f6923 stopped before execution: the AOT parser rejects reverse
static-property reference assignment (Unexpected token Ampersand). The fixture
now creates pre-existing aliases with supported $first =& Class::$text and uses
runtime-selected eval inside each native destructor for reverse rebinding. PHP
confirms identical four-case stdout. Both outer native and opaque Reflection
paths remain unverified, and no binding-identity bug has been claimed yet.


111 retry1a222096 stopped during lowering, before native execution: adding opaque
rebind inside the native destructor requires the staged helper to inject the
real eval/object-cast preludes. The direct case now also uses the eval-capable
fixture helper. This is harness setup, not a confirmed publication failure.

111 real baseline1fc635b0 selected exactly one regression and ran with a clean
heap. Native Reflection setter results differed from PHP only after rebinding
a pre-existing property reference: the old alias and property both became new,
where PHP leaves the property bound to second. Confirmed native eval reference
binding uses static_property_set plus an interpreter-only alias overlay. Work
in progress adds scoped physical native rebinding with a rooted staging cell,
shared declaration validation, and protected cleanup. This result does not yet
admit Casts.

111 current checks: the owning-alias EIR assertion passes for both Casts and
Coercions on all five targets; Clang assembles the new physical rebind, sharing,
and captured-destination helpers on all five targets. The existing protected
commit publication-order test passes. Native alias detachment/write-through
passes for Casts and Coercions with both IR modes, four clean heaps (135.79s).
The full Reflection identity/retirement regression is still running. New
sharing helpers distinguish native cell owners from direct canonical wrapper
owners and exclude validation/collector leases.

111 native Reflection cases now pass with both IR modes. The opaque outer
Reflection baseline remains red with clean heap778/778: its temporary argument
is retired after publication (SDnew) instead of before (SDold). Work in progress
shares exact evaluation-owner provenance through named/positional binding and
adds a consuming native class-setter entry using the same static-call publisher.
No remaining feature has been admitted by this pending work.

111 retry c631b74f stopped at compilation before tests: one native descriptor
CallArgumentModes initializer needed the new optional retirement field. Fixed
via the existing Default initializer. The current872db101 prebuild succeeds;
Clang accepts both capability paths and the consuming class-store ABI on all
five targets. Its opaque Reflection case now also uses reordered named
arguments. No full suite was run.

Future112 public Casts source/eval/callable/library checks are prepared in the
currently unregistered tests/codegen/strict_casts_public.rs and shared scalar
fixture tests/fixtures/strict/cast_values.php. They remain unrun. Current source
inputs are frozen while872db101 executes. Both native static promotion and
Reflection source-owner metadata are gated by the shared neutral profile
predicate, preserving ordinary source-argument ownership outside checked
property-reference profiles.

111 verified native reference/Reflection checkpoint872db101 exited0 after4m49s.
The real native/opaque Reflection fixture now matches all four PHP identity and
throwing-destructor cases with both IR modes, four clean heaps (142.79s). Opaque
invocation also verifies reordered named argument ownership. All-five-target
Clang assembly includes both static binding and consuming class-setter entries.
Public scalar, source-diagnostic, callable/eval, library, example and REPL tests
are now registered for the final Casts admission pass. No commit yet.

Public db4bad2c currently passes source evidence/independence, native+opaque
scalar domains (63.04s), native/eval callable hosts (60.26s), all-five-target
library emission (0.76s), and example (0.42s). REPL, selector control and final
build are still running. Casts admission additionally retains the explicit
historical alias audit: tests/codegen/strict_cast_argument_aliases.rs is prepared
but not registered while sources are frozen. PHP oracle verifies current-cell
mutation versus unset/rebind selection for native and eval callees, using
explicit mixed eval-owned properties and statics to isolate identity from
implicit conversions. Expected: Ninteger:7|integer:7;Ninteger:42|string:7.9;
Einteger:42|string:7.9;Ninteger:9|integer:9;. No compiler result exists yet.

Public db4bad2c also passes the real cached REPL (47.26s), then stops at
protected static selector generation: the new binding label mode needed the
getter ABI's scope-offset mapping. Added binding to both target mappings. This
was a compiler unreachable guard for non-public slots, not a numeric cast
failure. Pending-profile regression tests now select still-staged Comparisons
instead of candidate Casts. Job1ae14843 queues those narrow controls, the newly
registered argument-alias audit, and the final compiler build.

### Remaining Casts alias and source-coercion evidence (111, still pending)

Jobd65948d3 confirms the mixed alias failures again with a clean207/207 heap,
and typed float controls also lose their declaration: actual
Ninteger:1|integer:1;Einteger:1|integer:1; versus PHP
Ndouble:1|double:1;Edouble:1|double:1;. Current eval property scalar validation
is Coercions-only. Extending stable captures alone would not preserve the
weak PHP declaration rules needed by independent Casts. Native and eval
property sources must inspect the same original RHS before publication; do not
sequentially pre-coerce eval sources before native union validation.

The proposed-static-binding probe separately returns Tobject:X:old before any
Stringable hook (clean48/48); PHP returns Sstring:new:new even though __toString
replaces its unconstrained source with an array. The initial failure is the
weak native guard's static-only Stringable class list. Pendingf740df6b adds
ObjectPlan.eval_stringable, uses the existing live Stringable relation query,
and shares relation-ABI materialization with ordinary object_is_a lowering.
The established string conversion ladder already has an exception-safe erased
object fallback. A new EIR test covers all five targets with/without eval.
No native result for this guard fix yet. It is NOT yet established that the
temporary native source registration rejects reentrant mutation too early.

The re-entry probe f740df6b confirmed that issue (STobject:X:old versus PHP
Sstring:new:new, clean59/59). Implemented validation of a proposed
NativeReferenceTypeSource passed by value snapshot without installing it in the
live weak registry. Add a versioned validation ABI while preserving v1. The
coordinator must snapshot/authenticate existing native sources plus the proposed
source before ANY PHP callback. Proposed sources use a reserved token distinct
from registered generations. For Casts-only eval declarations, an optional
protected external-candidate callback can snapshot all live eval property
source declarations and coerce each from the same original RHS; existing
Coercions stays exact. Compare the external candidate with independently
coerced native/proposed candidates before commit. Weak eval normalization must
not feed into native union selection, and prospective constraints must not
become live during __toString. Version two preserves the existing version-one
entry and operation layout, adds an optional external source provider, and
authenticates copied proposed sources without registering them. Jobfd456b9d
passed all four Rust coordinator controls, all-five-target assembly, the real
proposed-source re-entry regression, both Reflection identity/retirement IR
modes (164.33s), and the previous exact eval/native assignment-point regression
(72.03s). The complete job exited0 after8m44s. No commit or Casts admission yet.

Job203e6ef8 now queues stable Casts source/property argument captures and a
shared eval-source candidate planner used by fake RuntimeValueOps and the
version-two native external provider, without double adaptation. New neutral
weak_scalar_binding plans implicit scalar preference and range rules separately
from checked explicit numeric casts. Immutable source decisions precede PHP,
and each candidate uses the same original RHS. Physical eval sources remain
distinct rather than deduplicated by declaration. Declared mixed contributes
an identity constraint, unlike an untyped property (reconfirmed with PHP and
existing CJ fact2f43cec8). Native/eval cross-source tests cover both mixed and
numeric-union conflicts plus removal of a source. This implementation remains
unverified until the queued job completes.
Preserve implicit PHP scalar preference/range rules separately from explicit
checked-cast kernels, and keep every owner/Throwable boundary protected.

Job203e6ef8 stopped at compilation after16s: the new candidate finisher passed
Result<Option<RuntimeCellHandle>> to the existing owner cleanup helper, which
accepts Result<RuntimeCellHandle>. Corrected the owned result before mapping to
Some and removed redundant exact validation after the shared planner. No runtime
test ran in that job. Casts with eval_bridge now explicitly selects native
reference validation even when there are no statically declared user properties.

Joba00480ba queues the corrected build, four neutral scalar planner tests, weak
and existing exact fake-reference controls, all-five-target weak-provider and
assembler checks, native argument-cell selection, independent native/eval mixed
and union constraints, proposed source reentry, the previous exact native/eval
assignment control, and a final compiler build. It also adds the PHP-verified
weak_property_reference_sources.php fixture: distinct instances of one property
each invoke Stringable conversion even after the first source retires, and a
throwing E_DEPRECATED precision handler escapes as TypeError with the original
RuntimeException as previous while storage remains unchanged. Oracle output is
SS:new:new;PTypeError:RuntimeException:1:1 (CJb882f71e and458a9819). The bounded
previous-exception builder roots constructor operands and retires every owner.
These new runtime paths are implemented but not yet verified. No111 commit yet.

Joba00480ba passed build, all four neutral weak-scalar controls, weak and exact
fake reference controls, the five-target external-provider policy test, and
Clang assembly for both Coercions and Casts. The real argument fixture then
confirmed current-cell mutation and float constraints fixed, but unset/recreate
still reused the old captured cell: Ninteger:7 and Einteger:7 versus detached
integer42 with replacement string7.9. Heap250/250 clean; job exited1 after5m05s.
CJ9110a87e records the concrete cause: the eval unset fallback wrote null
through the existing backing hash reference after clearing only alias metadata.

Added an eval-owned backing-hash removal fallback to the existing protected
typed-unset boundary. It reuses the property hash-slot classifier, installs the
COW table before hash_unset, and performs no table writes after value retirement.
Fixed native slots retain their existing dispatch; ordinary native objects do
not acquire eval backing-storage permission. New source fixture controls cover
two distinct same-declaration sources, retirement during Stringable conversion,
precision-handler exception chaining, clone/COW aliases and destructor replacement.
The full PHP oracle output is now
SS:new:new;PTypeError:RuntimeException:1:1|42:42:7.9D:replaced.

Added a bounded execution-context lease for the external provider. Active native
callers borrow their existing context; a foreign/fallback heap context retains
one ABI owner through PHP callbacks. The current candidate remains rooted through
context cleanup, and any conversion/teardown Throwable is exported after releasing
the context without dereferencing it again. Pure Stringable probing and execution
recover an escaped object's owning context. Three ABI lifetime tests cover final
host release, an existing native activation and nested outer teardown. These are
implementation safeguards, not a claimed reproduced UAF. New job0dba0d5f/session92441
queues these tests, all-five-target unset emission/assembly, prior candidate
controls, all native alias/callback fixtures and final compiler build. No new
runtime result for these additions yet; no111 commit/admission.

Job0dba0d5f passed build and two lease controls, then stopped at an invalid test
setup: begin_teardown correctly refuses a context that still has ABI owners.
Corrected the fixture to release its host owner before entering outer teardown,
then acquire the nested activation. Job9292c9c6 passed all three lifetime tests,
all scalar and fake exact/weak controls, both five-target unset tests and both
coordinator policy/assembler controls. Its native alias fixture then failed after
the first case with heap memory exhausted (3m11s, exit1). The new unset fallback
passed the hash only in rax, but the x86_64 COW helper consumes rdi and returns
rax. Added explicit ABI argument materialization and a five-target structural
check for the x86 input register (CJ948b621d). The separate inspected older direct
dynamic-property unset path is tracked as task1fffe7bc for reproduction; it is
not claimed fixed here. Current joba34a3a4e/session72535 queues the corrected
unset execution and all remaining reference/callback controls plus compiler build.
No new runtime result for that correction yet; no111 commit/admission.

Joba34a3a4e passed the real argument selection fixture (82.16s, both IR modes,
clean heaps) and independent native/eval mixed and union constraints (68.42s).
The callback fixture then stopped at eval parsing: it used reverse property-to-
variable reference syntax that the existing eval parser does not support. Changed
those two setup bindings to the already supported property =& variable direction,
with the same PHP oracle output, and added a fast parse assertion to the existing
fake candidate control. No parser language extension was added.

The builtin architecture audit passed:828 registry AOT builtins,484 compiler
routes, zero structural errors and no backend-dependent homes. I omitted
ELEPHC_DOCS_PREBUILT=1 and the audit implicitly ran cargo outside the queue;
this was an execution-protocol mistake, not a source change. It finished before
the attempted stop. Job5bb61656 then explicitly rebuilt the default Magician
archive through the queue and passed callback/retirement/COW/destructor/previous
exception controls (62.72s), proposed-source reentry (63.33s), exact native/eval
assignment points (71.53s), and compiler build (2m04s). Whole job exit0,5m54s.

Final reference-boundary review restores the direct interpreter's current-context
visibility now that publication delegates to the native coordinator. Checked
RuntimeValueOps reference replacement installs a context-only native activation;
the registry includes its current borrowed context even during outer teardown.
Inactive tearing-down contexts remain excluded. A unit control covers this
borrowed-stack case. This is a bounded restoration of the earlier current-context
validation path, not a claimed reproduced native crash.

Current job834c4b03/session74052 queues final public Casts source, scalar, callable,
five-target library, example and REPL controls; Reflection binding/retirement;
native/eval aliases and callbacks; existing exact and default native property
alias tests; the current-context controls and compiler build. CLI docs now list
Casts with the accepted features. No111 commit yet. Full plan remains active
after Casts, including stronger Locals, Comparisons and all remaining flags.

Read-only preparation for the subsequent Comparisons audit: builtin comparison
selection already lives in the neutral comparison_policy contract (identity
searches and explicit comparator alternatives). Verify native value-class
ordering cannot bypass the original-kind gate. Also probe comparator return
kinds and ownership before admission: eval_user_sort_compare currently delegates
to borrowed eval_int_value, which coerces to int and releases only its cast
temporary; the native sort adapter accepts Bool directly and casts Mixed results.
No native failure or fix for those observations is claimed yet. Keep the public
admission gated until focused source/runtime/callback proofs close that audit.

### Checked Casts admission (111, verified)

Final job834c4b03 exited0 after16m33s. All17 selected tests passed, each with an
exact single-test filter. Public source evidence/independence, native and opaque
numeric domains (71.61s), callable hosts (72.10s), five-target library emission,
example and real REPL recovery (53.05s) passed. Reference argument selection
(91.63s), independent native/eval mixed and union sources (87.51s), callbacks,
precision exception chaining, clone/COW and destructor replacement (80.61s) passed
with both IR modes and clean heaps. Native/opaque Reflection identity and source
retirement passed all four cases in both IR modes (161.67s). Static rebinding
alias owners passed Casts and Coercions with both modes (169.55s). Existing exact
native/eval reference controls (70.47s), default typed native aliases (44.03s),
current-context activation and pending-feature controls passed. Default Magician
and compiler builds passed. Only the two previously documented inherited Rust
warnings remain; no new warnings were introduced. Earlier five-target structural
and real assembler checks cover the native coordinator and unset fallback.

Casts is now the ninth accepted semantic feature. Source numeric conversion
contracts are enforced through shared kernels; implicit declaration rules remain
independent. Version-two source validation preserves version one, independently
compares native/eval/proposed candidates, and publishes only after all checks.
Physical static rebinding, exact argument retirement, eval capture/unset, context
activation and ownership helpers close the reproduced admission regressions.
The complete umbrella and remaining seven feature admissions are unfinished.
Continue stronger Locals and the Comparisons audit before nullability, mixed,
initialization, arrays, indexing and final cache/host/combination acceptance.


### Comparator callback admission audit (112, in progress)

Added two focused Magician probes before production changes: actual integer
callback returns must retire all sort entry/result owners in permissive and
checked modes, and strict comparator results must retain their original integer
kind. The probes cover usort, uasort and uksort; the ownership control also
reaches builtin fallback from a bytecode user caller. Baseline job ebb80c30 is
queued behind the existing Elephc docblock and WordPress verification jobs.
No runtime outcome is claimed yet. Reuse the existing sort entry/root/container
helpers instead of adding a second ownership lifecycle. Keep original source
policy authoritative for native adapters and maintain all five target paths.


Step112 baseline probes reproduced an outstanding Int(0) owner after permissive
usort and acceptance of a boolean comparator return under Comparisons. Existing
compiled single-unit replays established both failures without compiling or
running a heavy suite outside the queue. Generic sort entries now share the
existing collector, root inventory and partial-result builder; callbacks retain
an explicit result owner and checked consumers reject noninteger original kinds.
Direct writeback uses the existing lvalue/replacement finishers.

Job7907229b passed all seven exact unit gates and the default Magician build
(exit0,23s). This includes both caller entry modes, all three user sorts, five
rejected return kinds and failed insertion at each result position, plus ordinary
sort/key-sort/shuffle regression controls. Two earlier fixture versions stopped
at unsupported VM syntax; the corrected probe enters through VM and executes
builtin fallback inside the walker callee. The uksort control uses strcmp rather
than FakeOps permissive numeric-string ordering. Only inherited warnings remain.
Current jobc6848415/session35840 runs exporter/docs audits, restores the default
Magician archive, executes the opaque native-runtime retirement fixture in both
optimizer modes and builds the compiler. No112 commit yet. Native source-policy
metadata for subsequent step113 is being prepared separately and remains
uncommitted, with no native comparator guard claim yet.


The first opaque fixture attempt exited UnsupportedConstruct before any output.
The production eval FFI explicitly rejects pending semantic profiles, including
Comparisons; staged native lowering bypasses only frontend admission. This is
an admission guard, not evidence of a parser defect. The echo expression-list
syntax is supported by the parser. Added a fast parse assertion to distinguish
these layers. Queued job7d812690/session56173 will check parsing and still meets
the production admission guard until Comparisons is admitted. Do not claim a
native heap result from that blocked probe. Move the opaque execution fixture
into the later complete Comparisons admission verification.

Next native integration keeps the invocation policy in ProfiledFunction EIR,
so internal physical consumers cannot acquire user strictness from the emitter.
Runtime adapters must inspect the original boxed comparator result before any
integer cast, retain it through a catchable failure and support every target.
Descriptor, runtime-string and static callbacks must share the checked result
adapter. Comparator-based set operations need the same policy at their runtime
boundary. Public admission remains gated until those native and host controls
are verified. The other seven pending feature admissions and stronger Locals
remain part of the current user request.


Job7d812690 passed the exact parser/integer-kind unit, then stopped at the
expected production pending-profile guard in the checked opaque test (exit1,
2m22s). The shared-owner integration test now uses a separate permissive PHP
fixture; the checked-result fixture remains parse-verified for later admission.
PHP oracle for the permissive fixture:1,2;1,2;1,2;label-1,label-2.

Prepared step113 native integration retains strict_comparisons on ProfiledFunction
EIR, preserves it through optimizer rewrites, and shares one checked return
adapter across static/descriptor/string sort callbacks. A borrowed/owned native
guard checks the original unboxed integer tag before conversion. Comparator set
operations carry an explicit sixth ABI policy argument and retain their ordinary
cast path for internal/permissive invocations. New tests cover five-target EIR
and runtime emission, real cross-assembly, callback result kinds, descriptors,
string dispatch and heap retirement. These changes are not verified or committed.

Queued job38a0a3a1 was terminated by signal15 while waiting, without running any
tests; cause unknown. Replacement jobdf0ec42d has a persistent task-owned log in
target/strict-comparator-checks-_r_y5j68, wrapperPID3698701. It queues three exact
Magician regressions, the permissive opaque native heap control, all-target
native guard/EIR/assembler tests, native callback/set-operation runtime control,
and compiler build. Remove this exact task directory after collecting evidence.
Registered compiler sources remain frozen during the job. build.rs recursively
hashes every Rust file under src, including files not registered as Rust modules;
new unregistered source files are therefore not outside compiler build inputs.
No112 or113 commit yet; complete profile and stronger Locals work remain active.


### Eval comparator return and owner foundation (112, verified)

The shared eval sort collector/root/result builder now supports both precomputed
sort keys and user comparators. Callback returns are retired independently and
staged Comparisons consumers require an original integer result. Existing direct
lvalue finishers balance callback/target/replacement ownership. Seven exact unit
gates and the default Magician build passed in job7907229b. The three new unit
gates passed again in current jobdf0ec42d, including the checked fixture parser
assertion. Exporter generation and all builtin-docs audits passed in queued
jobc6848415 without generated changes. Only inherited warnings remain.

This is a component foundation commit, with Comparisons still publicly gated.
The production checked opaque fixture meets that intentional admission guard;
its runtime heap validation remains part of the subsequent complete admission.
The permissive opaque control and native callbacks are still running under
jobdf0ec42d. Step113 native policy/guard changes remain separate and uncommitted.
