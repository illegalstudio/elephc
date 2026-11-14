# Strict semantic profiles

- [x] Inspect CLI, source profiles, checking, folding, EIR, runtime, eval, and caches.
- [x] Record the agreed umbrella, PHP-only naming, and exact string-key rules.
- [x] Adopt the detailed boundary defaults from the approved implementation plan.
- [x] Add the shared semantic contract and resolve CLI requests once.
- [x] Rename the existing PHP-only option and its maintained integration surfaces.
- [ ] Propagate the immutable profile through compiler phases and runtime contexts.
- [x] Complete strict-dynamic-code source, execution, and REPL boundaries.
- [ ] Implement numeric rules, checked conversions, and catchable integer overflow.
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

Verified through step 47 (this commit), following `de9ba02ca7`. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask still contains only
Locals, Bools, DynamicCode and Opcache. Existing Locals behavior remains available
but its stronger numeric/reference/eval binding contract is outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

This progress ledger replaces the long chronological narrative after step 38.
The detailed evidence remains in Git history, Code Journal plan revisions through
56, the linked work logs, and the current agent-facing architecture document
3739892b. The approved behavior and delivery requirements above are unchanged.

### Verified foundations

- The dependency-neutral elephc-semantics crate owns the immutable ABI1 profile,
  sixteen-feature catalog, independent request resolution, scalar decisions and
  exact key classification. Specific positive/negative requests override the
  umbrella regardless of order, with last-wins explicit requests. PHP-only naming
  has no legacy CLI alias.
- Profiles travel through frontend options/results, EIR modules and contexts,
  backend/runtime generation, eval contexts and host capability checks. Physical
  SourceMode, PHP strict_types and public builtin invocation contracts are separate.
  Internal implementation operations retain explicit reviewed PHP semantics.
- Positive and negative parsed-source/executable caches partition by effective
  semantic identity, PHP-only visibility and PHP version. Eval source provenance,
  disk headers and versioned host setters agree. Magician disk format is 30.
  Runtime build fingerprints include semantic source. EIR EvalSourceProfile uses
  a checked compact u32 so Instruction remains within its 112-byte budget.
- DynamicCode audits every syntactic branch, resolver/autoload/include boundary,
  recursively verified literal eval, EIR fallback, opaque execution and REPL.
  Statically eligible literal eval remains AOT; runtime source is refused.
- Bools covers known and dynamic conditions, short-circuit/xor/ternaries, loop and
  iterator conditions, predicate callbacks, walker/VM, native hosts and cached REPL.
  Owned predicate/iterator failures clean up through catches and destructors.
- String interpolation still lowers to ordinary concat tokens. Preserve explicit
  formatting intent before admitting string-only concatenation.

### Verified numeric implementation

- Exact signed literal provenance distinguishes deliberate floats, integer
  overflow and the minimum signed magnitude. Both compiler/eval lexers and parsers
  retain the required source evidence, including interpolation offsets and caches.
- Compact ArithmeticPolicy travels explicitly in NumericOperation, NumericUpdate,
  NumericUnary and NumericFunction targets. Native capability does not select an
  operation's semantics. Module validation, effects, ownership, printing and
  optimizer transformations retain the policy.
- Scalar IOverflows guards, boxed exact operand classification, integer power,
  division/modulo, unary normalization and updates preserve PHP kinds. Arithmetic
  and Coercions independently guard original numeric domains, while Overflow
  independently changes integer promotion into catchable ArithmeticError.
  Negative integer powers are rejected only by Arithmetic. Bit operations retain
  their reviewed wrapping/shift contracts. IEEE floats, signed zero and NaN bits
  remain governed by their explicit operations.
- Source validation precedes folding, retains full numeric-string provenance,
  declaration/default/call evidence, named/spread planning and constant cycle
  guards. AST effects and exception-flow summaries preserve warnings, user callback
  writes and discarded failures. Auditing typed evidence erased by the initial
  fold remains a required completion check before feature admission.
- Unary plus is an explicit AST node, traversed by every structural visitor and
  rebuild/default/reflection/synthetic adapter. Typed EIR unary normalization uses
  independently owned boxes and preserves native object precedence. Direct scalar
  identities require matching logical kinds and physical representations.
- Pow shares the operator graph for every active numeric policy. Native class
  result contracts, original kinds, exact integer results, named/spread/FCC and
  indirect calls agree across frontend, native and eval execution.
- NumericParameters descriptors preserve original builtin arguments before binding
  can coerce them. Prepared public wrappers and direct calls share rooted EIR
  guards; Internal and permissive signatures remain unchanged. Abs accepts int or
  float under numeric domains, while intdiv requires int. Intdiv validates each
  known parameter even when its companion is unknown. Its value failures remain
  catchable in all profiles.
- Sum/product use typed NumericFunction policy for every numeric guarantee.
  Profiled native entries receive numeric bits as a third internal ABI argument,
  held in a frame slot checked against the exception record. They guard elements
  after explicit native to_number adapters, select checked versus promoting
  combination from caller bits, and share the existing source snapshot/unwind
  loop. PHP and Overflow-only entries retain their old ABI.
- Aggregate checker/source proofs retain concrete element contracts, partial final
  entry evidence, overwrite/key identity and native Number casts. An arithmetic
  object overload alone does not authorize numeric aggregation. Eval snapshots
  the array and roots source, element, numeric view and partial carry. Identity
  numeric views are borrowed; fresh casts and temporary iteration keys are retired.
- Outlined boxed operators partition trusted Internal and user callers when a
  profile is active. Only public builtin invocation adapters carry the builtin
  wrapper flag. Native comparison dispatch uses an explicit Internal contract.
- Exact typed literal-key planning preserves original string bytes/kinds and
  append provenance in source reduction proofs. This is groundwork; the complete
  runtime key, producer, array type and indexing guarantees remain outstanding.

### Verified implicit scalar parameter groundwork

Step40 adds a shared scalar-contract adapter over the neutral conversion planner.
It preserves exact scalar union members and distinguishes literal false from bool.
Known invalid user PHP/LFC bindings fail before permissive assignability; Internal
operations retain their reviewed behavior. Static unpack proofs reuse the shared
spread expansion and named/positional planner instead of matching parameters again.
PHP strict_types remains independent, including its ordinary int-to-float exception.

EIR argument evaluation retains original values until every caller expression has
finished. The parameter-order pass roots all operands, including an allocated
variadic collector, before kind guards and representation-only extraction. It
suppresses literal binding rewrites and destination casts that would erase kinds.
Selected descriptor guards use exact declaration membership. Eval's common dynamic
and native parameter binder rejects coercion after full-type exact acceptance.

AST effects and exception-flow boundary queries add the independent binding
TypeError outside callee summaries. The native regression reproduced a pruned catch
with optimization enabled before this fix and passes after it. New neutral,
selected frontend, five-target EIR/emitter, walker/VM call-boundary and native tests
pass, with both optimizers and clean ownership. VM probes register typed callees
through the walker and use a dead untyped shape in their compiled caller, preserving
the existing ParameterType admission gap rather than claiming a typed fast path.
The initial-fold tests also confirm typed pow/intdiv violations in ordinary dead
branches survive until checking. Five focused compatibility controls, the compiler
build, architecture gate, assembly comments, PHP syntax and diff checks pass.

This is parameter groundwork, not admission of Coercions. Returns, assignments,
property/reference write-back, defaults, every builtin contract, array element
adaptation and all indirect/host boundaries still require completion and audit.

### Verified declared scalar return groundwork

Step41 retains source mode and source position on ReturnInfo after body checking
restores its surrounding scope. Functions, methods, closures and arrows share the
same pre-compatibility noncoercive checker helper as parameters. Declared dynamic
scalar/union returns use original-kind guards before return storage adaptation;
Internal returns and inferred returns retain their existing policy. Eval's shared
declared-return path rejects coercion after full-type exact acceptance and releases
rejected result owners. ReturnBoundaryFacts uses the same plan for effects,
exception summaries and the checker's source-point exception edges.

The shared scalar adapter now uses existing typed CastTarget storage paths after
a guard. MixedUnbox is a pointer-payload ABI, not a full F64 or string-pair adapter.
The native regression was red with a NUL string result before this correction;
new assertions verify float payload 4.25, negative zero, actual owned string bytes,
and the corresponding parameter ABI (fact7a44bb20).

Two frontend/all-five-target units, one walker/VM return-boundary unit and one
native regression pass. Native coverage includes failed destination preservation,
same-frame catches, finally, discarded results, FCC/call_user_func/arrow/array_map,
false/null unions and fresh string owners, with both optimizers and clean heaps.
The strengthened parameter regression, its two frontend units, an all-target
object-return owner control and PHP strict_types widening control pass. Compiler
build, architecture gate, assembly comment checks, PHP syntax and diff checks pass.
Public admission remains unchanged. Inferred result kinds, assignments, defaults,
property/reference contracts, builtin source folds and host parity remain active.

### Verified inference across separate scalar returns

Step42 introduces a shared scalar union join under Coercions. Free functions,
instance/static methods and closures retain the exact alternatives observed at
separate return statements. It reuses canonical union normalization; arrays,
nominal types, default mode and trusted Internal inference retain their existing
join rules. This prevents a large int branch from rounding through an inferred
float ABI. Conditional-expression and local-flow joins remain a separate audit.

A private native-module test helper executes modules built from the actual staged
checker result. It writes separate user/runtime assembly into one owned directory,
links bridge-free fixtures with the supported host C toolchain and removes every
owned file through Drop. It does not weaken public profile admission. This gives
native evidence for changes to checker-selected representations, which the older
permissively checked staged integration fixture cannot establish.

The new five-target unit covers default/user/Internal inference and both EIR
optimizer settings. The native unit covers functions, both method kinds and a
closure, preserving int 9007199254740993 versus float 2.5, with clean heaps both
without optimization and through the AST plus EIR pipelines. Both declared-return
frontend controls pass. Compiler build, architecture gate, assembly comments,
PHP syntax and diff checks pass. No admission mask or runtime ABI changes.

### Verified conditional scalar and closure result contracts

Step43 preserves canonical scalar unions for ternary, short-ternary and match
results. CheckResult.closure_result_types carries inferred scalar/Mixed closure
contracts by enclosing body and closure span; it avoids a second scalar-biased ABI
guess. Conflicting visits choose Mixed storage. Container layout stays with the
existing parameter/capture-aware materializer, and bound-receiver specialization
keeps precedence. The renamed closure_hints module scopes parameter and result maps
together, restoring nested lowerings. Result lookup requires the explicit active
Coercions profile and user SourceMode so Internal closures retain their own policy.

The native test was red on step42: arrows rounded 9007199254740993 through F64,
converted true to string and null to an empty string. All five new checks pass:
four conditional/source-identity regressions plus the nested scope/mask unit.
They cover all target emitters, PHP/LFC, both AST/EIR modes, captured fresh strings,
function-local arrows, conflicting spans, and overlapping trusted/user spans, with
clean native heaps. Five focused inferred/declared-return and AST-smoke controls
pass. Compiler build, architecture, assembly comments, PHP syntax and diff pass.
No eval cache format, runtime ABI or public admission change is needed. Local-flow
joins, default/assignment/property/reference contracts, builtin folds and complete
host boundaries remain active completion work.

### Verified declared scalar initializer contracts

Step44 validates original scalar initializer kinds before weak declaration
compatibility and before EIR storage adaptation. It reuses the parameter/return
implicit conversion plan and catchable TypeError guard, preserving an earlier
binding on failure. Nullable integer storage uses the existing tagged-scalar
adapter after validation, with the replaced owning Mixed cell released. The
runtime fallback now shares the same target-aware adapter as call argument
materialization instead of carrying a narrower duplicate conversion switch.
AST effects and exception summaries retain initializer TypeError independently
of RHS effects, including a pure local read in a same-frame try/catch.

The original native probe was red on step43: invalid kinds passed, a string
payload disappeared and a float was corrupted. The completed regressions cover
known invalid sources, explicit conversions, all five emitters, exact large
integers, F64 values, owned strings, bool, nullable and false/null unions. Native
isolated nullable cases and the combined program verify clean ownership and
both full AST/EIR optimization settings. Untyped numeric branches already kept
both original kinds and required no speculative flow rewrite. Subsequent stores,
conditional binding contracts, defaults, properties and references remain active.
The four parameter/return frontend controls and both native controls pass.
Compiler build, architecture gate, assembly comments and diff hygiene pass.

### Verified subsequent declared scalar stores

Step45 records original declared scalar contracts in CheckResult.local_store_contracts
by body, source span and local name. Checker re-visits replace their own facts;
read narrowing and physical storage do not select the write contract. Successful
runtime membership checks publish the validated destination type for later reads.
The immutable map is passed explicitly through function, method, closure, eval
and synthetic lowering contexts. Generated bodies without source facts receive an
empty map. conversion_boundaries::local_store shares validation and storage
adaptation for statement and expression writes before any destination coercion.
Optimizer effects and exception summaries retain implicit store failures.

The regression was red on step44: a float declaration accepted an int store,
and native writes replaced prior float/string values without TypeError. Three
new tests pass with known diagnostics, default/Internal controls, authoritative
read facts, all five emitters, native statements/expressions, function/closure/
method scopes, narrowed nullable stores, scalar unions, failure atomicity and
clean heaps through both AST/EIR optimizer modes. Three initializer controls pass.
Compiler build, architecture gate, assembly comments and diff hygiene pass.
References, alias-origin contracts, defaults, properties, eval locals and the
remaining strict families still require completion. Public admission is unchanged.

### Verified proven scalar declaration defaults

Step46 adds one shared AstVisitor::declared_value observation for parameter,
property and typed-constant values. A focused schema adapter validates proven
scalar kinds after class-like constants are available, using each physical
statement's SourceMode and lexical class instead of the schema pass's Internal
policy or a future caller. Existing default resolution is extracted into a
reusable original-kind query. Raw defaults first use the existing lexical
normalizer so unbound object factories retain deferred Error behavior. The kind
query does not duplicate existing deprecation warnings. Unresolved/deferred
values retain their established validation path.

An unused float parameter with integer default compiled on step45; the new
regression rejects it independently of invocation. Three tests pass across
fourteen declaration forms/constants, PHP/LFC, strict_types=0, permissive/Internal
controls, overlapping trusted/user sources, traits and lazy scope errors. Valid
defaults emit on all five targets. Native direct/FCC/closure/arrow/method/property
values retain actual F64 payloads and nullable defaults, while explicit arguments
bypass unbound defaults and omission still throws Error, with clean heaps and
both AST/EIR modes. Three adjacent store controls pass. Dynamic or conditional
default plans, trait receiver resolution, property/reference/alias/eval/host
contracts and all remaining strict families stay active. Public admission is
unchanged. Compiler build, architecture gate, assembly comments and diff hygiene pass.

### Verified scalar declaration evidence and nullable union effects

Step47 shares scalar_hint across source declaration evidence and optimizer return/
store adapters, including named null and false-only union alternatives. The old
adapter lost null's scalar identity: a pure int|false|null initializer kept its
catch without optimization but became an uncaught TypeError after DCE. All five
emitter checks now retain the catch and native execution stays clean in both modes.

The shared source proof engine observes declared values before folding, selects
known ternary/short-ternary/coalesce values, and retains string truth separately
from numeric parsing (so '0' and '00' remain distinct conditions). Direct trait
constants can supply self evidence; unresolved consumer-provided constants remain
unknown. Typed constant evidence projects the declaration's PHP result kind,
preventing false integer overflow for a float constant initialized from an int.
Four new focused checks pass, covering forward constant conditions, original
kinds, typed constant contracts, false/null contracts, all five emitters and both
native AST/EIR modes with clean heaps. All three default controls and the exact
numeric reduction/domain control pass. Compiler build, architecture gate,
assembly comments and diff hygiene pass. PHP confirms typed float constants return
float values and inherited trait defaults retain their original self constant.

A further native trusted-producer probe exposed an EIR constant materialization
gap: Internal const float VALUE=1 still arrives as an int in executable lowering.
PHP extension emission already has a schema materializer, but its current pipeline
invocation is extension-only. The red fixture is preserved separately in
src/ir_lower/tests/strict_trusted_constants.rs for immediate follow-up. Public
admission remains unchanged. Complete this shared producer boundary next, then
remaining dynamic/default/property/reference/alias/eval/host contracts and the
rest of the approved strict families.

### Native eval boundaries and ownership

Protected checked_numeric_v1, numeric_unary_v1 and warning_v2 hooks contain native
exceptions beneath Rust frames. Success transfers one owner; unsupported requests
leave outputs untouched; failures use pending Throwable ownership and status2.
Unary requests validate selector/capability subsets before reading pointers.
Numeric operands are snapshotted before warning callbacks; C ABI spills, operand
pins, normal call versus jump-entry stack alignment and cleanup must remain sound
on every target. No Rust unwinding may implement a PHP Throwable.

Step39 closes invalid checked aggregate sources. A native regression was red on
step38: sum/product receiving an int returned status0 with a null result. Shared
source diagnostics and normal call-entry failure helpers now produce protected
TypeError status2 before result publication, preserve the output slot and borrowed
input, and transfer the pending Throwable correctly. The new five-target emitter
unit and native regression pass, along with three boundary/owner controls, the
compiler build, architecture gate, assembly comment checks and diff hygiene.

### Completion work, in order

1. Audit the remaining numeric call/host paths. Verify initial folding cannot erase required
   numeric guards or typed source violations. Close any concrete remaining literal,
   compound-store, update, callable, reference and exception ownership gaps.
2. Integrate shared conversion plans for implicit assignments, parameters, returns,
   callbacks, descriptors, reflection and eval re-entry. Complete checked explicit
   casts with range/domain errors, exact strings and native object cast contracts.
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
| 47 | this commit | feat(strict): preserve declaration kinds and union failures |
