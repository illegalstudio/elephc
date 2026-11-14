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

Verified through step 75 (this commit), following `180fbf3a6c`. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask still contains only
Locals, Bools, DynamicCode and Opcache. Existing Locals behavior remains available
but its stronger numeric/reference/eval binding contract is outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

This progress ledger condenses verified implementation through step 75.
Detailed evidence remains in Git history, historical Code Journal plan revisions,
the linked work logs, and the current agent-facing architecture document
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

### Verified value contracts and declarations

Steps40-52 share exact scalar, nominal and structural membership through the
neutral conversion planner, full compiler type adapters and typed EIR guards.
These contracts distinguish false from bool and exact alternatives from physical
boxed storage. Invalid known PHP/LFC bindings fail before permissive assignability;
Internal operations retain their reviewed behavior. PHP strict_types remains
independent, including its ordinary int-to-float exception.

Parameters, declared and inferred returns, conditional closures, local stores,
initializers, defaults and public descriptors retain original source kinds.
Runtime guards run after source-order operand evaluation with owners rooted,
before coercion or representation extraction. Each declared local store consumes
CheckResult.local_store_contracts instead of success-only narrowed flow facts.
Exact union membership includes nominal objects, interfaces, callable descriptors,
arrays, iterables, resources and structural contracts. Reusable condition graphs
preserve their original rejection class and noninteger union alternatives.

Steps53-57 retain producer expressions rather than only prepared values. Original
call proofs consume CallArgPlan, static associative spread expansion and actual
trait/inherited-constructor signatures. Expr.source_profile and producer_binding
survive copying and folding. Producer binding occurs once under its physical
source policy, including declaration defaults, omitted arguments, property
initialization, class constants and global constants. Identity and legal widening
have explicit binding metadata; lowering does not globally override synthetic
expression policy. Raw defaults remain available for reflection.

GlobalConstants stores (Expr, PhpType), not an isolated ExprKind. The same declared
constant materializer serves globals, classes, local consumers and public default
arguments, preserving signed zero, strings and conditional original kinds. Resource
alias constants keep their special handling. Effects, exception summaries and
frontend walkers preserve all required failures before initial folding.

Native verification covers both optimizer settings and clean heaps, with selected
profile checker/EIR evidence for every supported target. Staged integration
fixtures currently check permissively and then select the module profile; they
prove runtime guards, not selected-profile inference. The separate root tests use
the actual selected checker and remain required evidence.

### Verified explicit numeric conversions

Steps58-61 share NumericCastTarget/Input/Failure, checked_numeric_cast and exact
radix conversion kernels. Finite float-to-int truncation checks range before
conversion. Complete decimal strings preserve exact endpoints, fractional and
exponent truncation without a binary64 round trip. Full-string grammar rejects
junk and invalid bytes; explicit floating results retain IEEE infinity, NaN and
signed zero. Radix parsing validates complete digits before reporting range,
including PHP prefixes and base0 octal. Unknown string bases defer proof, while
nonstrings intentionally ignore the base.

Original-source validation runs before folding and retains strings in its evidence
pool. CheckedNumericCast and CheckedRadixInt EIR targets carry exact types,
capability requirements, effects and validator contracts. Target-aware runtime
entries share decimal/radix parsers and catchable failures. PHP string result
registers differ from C arguments, especially x86_64 rax/rdx versus rdi/rsi; the
ABI string_result_regs helper owns that materialization. Darwin failure branches
use an inverse local branch followed by a wide global jump. Real clang assembly,
PIC/dead-strip checks and emitter tests cover all five targets.

Magician walker and VM scalar conversions use the same pure Rust kernel.
native_results::finish transfers protected native Throwable ownership and keeps
success/unsupported states independent of pending errors. Reviewed object adapters
run before scalar conversion. Rejected results, source views and scratch owners
are retired on every failure. VM positional-call tests use an unreachable same-name
declaration to admit their shape; unsupported VM syntax is tested through the
walker. FakeOps has a separate permissive prefix/precision gap tracked by journal
task dc79bdf2, so controls do not claim unsupported PHP equivalence.

Step62 gives intval/floatval explicit cast semantic descriptors, a shared
BuiltinLoweringContext::emit_numeric_cast boundary and common rooted EIR graphs.
The optional intval base applies only to strings. CallArgPlan owns static named
and spread source proofs. Direct, first-class and indirect eval dispatch select
the same checked behavior before native fast paths. Checked conversions retain
MAY_THROW effects, source-order leases and original operand kinds.

Step63 converts numeric settype values before reference writeback. Potentially
failed ordinary local conversions retain tagged storage and conservative views,
so catch blocks still read the original value. Authoritative local contracts and
existing reference storage remain active. Eval with_direct_ref_target owns
nonescaping array keys/receivers through conversion, writeback and cleanup; its
owner capture must not be applied blindly to escaped generic reference metadata.
Known unsupported targets and dynamic target-domain parity still require audit.

Step64 adds CheckedNumericProjection to the shared native class contract. Final
internal adapters expose original GMP decimal digits and SimpleXML text before
legacy numeric adapters narrow them. The compiler and eval validate this source
with the shared kernel, retaining original receivers, projected strings and
adapter-result owners through failure. GMP floating conversion preserves get_d
truncation after source proof. SimpleXML bypasses public __toString overrides.
Typed intval/floatval calls share syntax adapters; AST usage and reachability root
the original source methods before EIR selects them. Real native preparation
requires core, GMP/XML, DOM, object-cast and function-argument preludes in order.

The permissive erased SimpleXML floatval fallback retains its preexisting public
string override behavior; journal task 693969cd tracks that separate baseline
mismatch. Checked profiles use original native text. Managed GMP/XML bundles,
walker/VM owner tests, all-target selected checker/EIR tests, compiler gates and
builtin-docs regeneration pass. The public mask remains unchanged until complete
numeric cast, settype, source and host guarantees are enforced.

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
   Original GMP and SimpleXML cast projections are now checked. Before Casts
   admission, finish settype target domains and host callbacks. Runtime gmp_intval range guards and broad
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

### Broad native cast domains

Step65 admits generic object, interface and nullable/union source domains to the
same checked native cast dispatch. Known concrete classes without a reviewed
numeric projection remain compile diagnostics. Unsupported runtime values in a
partial nominal domain throw TypeError before conversion. The frontend helper
preparing real GMP/XML/DOM declarations is shared by the independent regressions.
Six selected-profile checker/EIR tests cover the complete target matrix and known
invalid inputs. Two managed-native bundles exercise generic object, interface,
nullable, union and first-class calls with both optimizer settings, exact integer
limits, failed conversion, caller preservation and clean heaps.

### Checked public GMP integer projection

Step66 selects a separate compiled helper for the public gmp_intval contract under
Casts. Weak floats are checked before the existing operand adapter can truncate
them. The ordinary GMP validator keeps its parsing, strict_types and error rules;
its canonical decimal result must fit a signed machine word before get_si runs.
AOT direct/callable paths share existing argument boxing and unwind owners. Eval
selects the same checked helper through a typed values hook and the shared bound
native-call path. A host missing that helper fails closed instead of using the
legacy helper. Five selected-profile checker/EIR tests pass for all targets, two
eval tests cover walker/VM selection and host gaps, and three managed-native
bundles pass both profiles and optimizers with clean heaps, including PHP
strict_types error priority. Two existing GMP contract tests, compiler/test-target
builds, full builtin-docs regeneration/audits, example syntax, assembly comments
and diff hygiene pass. Generated metadata is unchanged. Original-source folding
proofs and complete host admission remain part of the outstanding Casts audit.

### Original public GMP source proof

Step67 adds a neutral conservative proof for canonical decimal GMP integer
sources and original weak floats. A dedicated builtin semantic descriptor marker
joins that proof to the original-source argument planner before folding. Known
decimal range loss and invalid scalar kinds are diagnosed through declaration
aliases, named/static spread arguments, removed branches, defaults and literal
eval. Proven integer results participate in independently enabled Overflow proof.
PHP strict_types domain rejection has priority, and Internal/source declaration
shadowing retain their existing contract. Native radix, whitespace and other
unfamiliar spellings stay unknown, validated at runtime by the existing compiled
helper; the source validator never substitutes intval grammar. Three neutral and
three compiler proof regressions cover these rules. All five selected-profile
GMP EIR/emitter tests and two existing source-cast controls pass, together with
compiler/check-tests/exporter builds, complete builtin-docs regeneration/audits,
assembly comments and diff hygiene. The generated registry records the dedicated
GMP cast proof contract. No runtime or public admission policy changes.

### Shared settype target validation

Step68 adds SettypeTarget and SettypeTargetFailure to the neutral crate. Complete
case-insensitive target names and aliases have one owner, distinguishing invalid
names from forbidden resource conversion and from valid nonnumeric targets.
NumericCastTarget::from_type_name, checker, EIR static lowering, legacy backend
classification and eval consume that catalog. Checked invalid runtime names raise
ValueError before writeback; source proofs identify the typed Settype runtime
inventory and use CallArgPlan before folding, including aliases, named/static
spread calls, defaults and literal eval. Permissive and Internal controls remain.
Valid array/object/null targets are classified but their backend conversions and
dynamic target parity remain outstanding; public Casts admission is unchanged.
The current native settype regressions exercise direct/by-reference calls. Add
explicit AOT first-class/indirect call evidence before claiming those paths from
the broader eval coverage. Seven new neutral, source, frontend and eval tests
pass, along with the focused numeric-cast and settype controls, including all
supported EIR targets and native conversion/owner tests with both optimizers.
Compiler/check-tests/exporter builds, complete builtin-docs regeneration/audits,
assembly comments and diff hygiene pass; generated metadata is unchanged. Journal
task 67c8492a tracks the separate permissive invalid-target mismatch, with the
existing valid-null and property-reference tasks retained.

### Prepared checked settype calls

Step69 replaces the checked callable's legacy constant-target backend with a
prepared EIR value conversion graph. Runtime scalar/null target names share the
neutral alias catalog; numeric conversion delegates the original-value cast
adapters. One preserve_binding helper retains failed original tags without
widening an existing reference ABI. Normal stores own writeback and retirement.
Source and normalized target owners remain rooted. The graph runs with an
independent write receiver so an enclosing assignment cannot suppress ownership
of its fresh intermediates. String conversion carries user/global effects.

The AOT callable regression was red on step68: a literal target at the caller
became a parameter in the generic wrapper, which rejected it as nonconstant.
Native first-class, string-selected and runtime-target calls now work. Explicit
call_user_func value contexts need copied cells rather than synthesized caller
references under the checked contract. Prepared builtin reference signatures
therefore retain descriptor invocation instead of static argument flattening;
shared value-container builders detach cells. Direct/native descriptor references
and actual references in prebuilt arrays remain live. Permissive/Internal routing
is retained. Array-literal reference syntax remains a target-independent parser
diagnostic; that limitation does not imply references in existing arrays fail.

Four compiler tests cover every emitter, both native optimizer settings (AST and
EIR), first-class/dynamic/call_user_func* calls, null/scalar aliases, invalid and
NUL-bearing target names, by-value copies, actual references, enclosing assignment,
string callback global effects and clean heaps. Prepared bodies also run with
ordinary numeric conversion in permissive catalogs. An explicit prepared-invocation
role keeps static callable inlining consistent with the generated body, without
selecting it for ordinary legacy direct/value-call lowering. One eval test covers null alias
and COW-container publication. Valid array/object targets retain an explicit staged
unsupported error until their conversions and helper dependencies are completed.
Public Casts admission remains unchanged. Focused compiler, Magician and neutral
tests, the real opaque-eval assignment regression, cargo check --tests, cargo build,
the curl exporter, generated builtin audits, assembly comments and diff hygiene
passed. The updated example also compiled and ran with Bools and DynamicCode.

### Complete settype collection targets

Step70 completes every valid target in the prepared settype graph. Array conversion
reuses the evaluated-value array cast lowerer; object conversion calls the existing
injected PHP graph with rooted original storage and hidden ABI argument support.
No extra synthetic PHP local extends the operand lifetime. Direct checked and
prepared permissive bodies share collection conversion while numeric policy remains
independent. The original value is published only after conversion succeeds.

The builtin home declares reusable PreludeHelper requirements. Shared declaration
usage distinguishes implicit dependencies from explicit calls to the same names;
the object-cast injector consults this inventory and dynamic callable/eval hazards.
Reachability retains dependencies from direct and first-class calls. User helper
names with no cast or implicit dependency keep their ordinary acceptance.

Eval collection conversion preserves existing object identity, uses ordinary array
casts, and builds stdClass from insertion-order values and byte-exact property
names. Every temporary and partial object has an explicit owner through failure.
String and boolean targets now use the common eval cast adapters, so eval-defined
Stringable effects and throws remain visible. Explicitly owned variable writeback
uses set_owned_scope_cell to retire an acquired identity-result lease, fixing the
leak exposed by the new object round-trip regression.

Two compiler collection tests cover all five emitters, dependency pruning, both
numeric profiles, EIR optimization, object identity, COW and native clean heaps.
Two eval regressions cover collection/ref/container ownership and Stringable
callback effects/failure. The existing checked callable and scalar controls are
retained. Public Casts admission remains staged while remaining host and reference
contracts are audited. Focused scalar/callable regressions, prior numeric builtin
controls, implicit dependency and reserved-name diagnostics, the real opaque-eval
assignment test, check --tests, build and the curl exporter passed. The example
uses an explicit mixed container for intentional array-to-object retagging.

### Scalar builtin conversions

Step71 shares boolval/strval semantic descriptors and prepared callable lowering
through the full explicit scalar cast adapter. Checked boolean conversion uses
reviewed native handlers while permissive IsTruthy behavior remains unchanged.
String results publish an independent owner, matching a Fresh semantic contract;
source arguments stay borrowed and rooted, and intermediate string owners retire
after persistence. Effects::php_conversion reuses the existing PHP callback
summary without marking the operation itself as a refcount instruction or direct
blocking I/O. The validator admits that complete cast summary as an explicit
alternative to legacy cast effects.

The new string regression failed on step70 with an undefined dynamic function.
After callable support was added, fresh-owner regressions exposed descriptor and
array_map leaks. One shared predicate now selects full PHP wrapper construction
and return-status consumption by its invoker; Prepared/Fresh string metadata also
reaches the callback ownership predicate. These root string regressions pass all
five emitters and both native profile/optimizer settings, with each call path
checked separately for clean heaps. Eval strval now uses shared operand leases and
the existing cast evaluator, which retires object-hook result owners. Its new
call-surface owner regression passes.

The checked shared Boolval hook uses the ordinary eval cast path. Boolean casts
adopt protected native failures before returning to a catch; an injected native
failure regression was red before this handoff. Native SimpleXML parity passes for both profiles and optimizer modes, with clean
heaps. Its complete matrix took 64.81 seconds and uses the bundle CI timeout.
The permissive native-handler mismatch is preserved and tracked separately as
6a5ed1e4. Eight compiler tests, two eval regressions and the effects validator
pass, as do the focused settype/numeric controls, six existing boolval cases,
strval dispatch, owned/borrowed descriptor-string cleanup, opaque eval (48.15 s),
check --tests, build and the curl exporter. Full generated docs and architecture
audits pass (828 AOT, 77 graph, 56 primitive, 695 runtime, 679 eval). The updated
PHP-compatible example compiles and runs with Bools/DynamicCode. Task temporary
directories are removed. No admission change is made before all Casts and
remaining feature contracts are complete.

### Contextual string warnings and diagnostic owners

Checked eval string conversions now dispatch array-to-string warnings through the
shared contextual PHP dispatcher before publishing their result. Syntax, strval
and settype agree on the warning, source-kind snapshot and handler ordering.
A handler may replace the original binding without changing the conversion result;
a throw prevents settype writeback. Protected native cast failures are adopted by
the common cast adapter before returning to either interpreter consumer.

The warning dispatcher has a focused core/error_dispatch.rs home. Four empty
RuntimeOwnerSlot fields are registered before constructing callback arguments and
filled incrementally. Callable invocation borrows those arguments, so the dispatcher
releases each independent lease after native failure handoff. Suspended handler
cells and returned values remain rooted through restoration and cleanup. This
closes the leaked callback argument owners exposed by the new conversion tests.

Four string-conversion regressions pass, including warning masks, suppressing and
throwing handlers, callback mutation and walker/VM conversions. The owner-slot
visitor test and all 17 directly affected core introspection tests pass. A real
opaque-eval handler/throw regression passes with both optimizer settings and a
clean native heap (57.72 s). It uses the admitted Bools profile: Casts remains
blocked at public eval entry until its whole contract is complete. Staged eval
fixtures now inject the ordinary implicit object-cast dependencies required by
the full builtin catalog. Assembly-comment checks, example syntax and diff hygiene
pass. Check --tests, compiler build and the curl exporter pass; regenerated
builtin docs and all architecture/site audits report no errors (828 AOT, 77 graph,
56 primitive, 695 runtime, 679 eval). Public feature admission remains unchanged.

### Live prepared reference places

Prepared checked mutators now share a scoped argument role in the existing call
planner. Eligible Mixed properties, static properties and array entries acquire
live managed aliases at their source position. The checker reuses its source-call
property promotion helper, so direct builtins do not depend on an earlier explicit
reference alias. Conversion graphs recover the original writable binding and use
ordinary reference stores; a detached property copy cannot replace this behavior.

Signature-unknown descriptor calls reuse the existing passing-mode metadata probe.
The generalized descriptor_argument_contexts module replaces the XML-only module
name and keeps its native selection behavior. It binds supported ordinary Mixed
places only on the selected by-reference edge; the by-value edge snapshots its
argument before later argument effects. RootedDescriptorCallback keeps argument
alias owners outside the argument container and retires them after dispatch. No
new runtime ABI or per-target emitter is introduced.

Six new compiler checks cover the selected checker, standalone property promotion
and all five target ABIs. The native shared fixture covers direct/prepared/runtime
callables, static properties, nested entries, COW peers, catches, by-value source
order and suppressing/throwing property warning handlers. Both AST/EIR optimizer
settings pass with a clean heap (9.93 s). The 18 focused settype tests, five existing
XML argument emitter tests, native XML materialization/ownership control (66.87 s),
check --tests, compiler build, curl exporter, assembly comments and diff hygiene
pass. Generated builtin docs, site compatibility and the EIR architecture audit
pass with unchanged inventory (828 AOT, 77 graph, 56 primitive, 695 runtime,
679 eval).

Casts admission remains staged. Narrower typed destinations, erased native/ordinary
reference contexts, public callable bindings from Internal producers and remaining
host boundaries still need their explicit audit before publication.

### Erased native-capable reference receivers

The native XML argument selector now preserves checked ordinary references on its
fallback edges. Root array variables use the shared evaluated-receiver element
binder, retaining the original local/ref-cell origin needed to republish COW
separation. Named properties select exact runtime classes with owned Mixed cell
metadata before binding a live cell. The typed object projection borrows from the
enclosing receiver guard, so access failures do not strand an extra object lease.
Receiver expressions and index effects are not replayed. Class-name and result
owners remain rooted through the exact-class dispatch and initialized-slot merge.

The original regression returned unchanged ordinary values when XML was present:
9.9:9.9|8.9:8.9|7.9. Checked execution now yields 9:9.9|8:8|7.9, keeping the array
peer and the independent XML view unchanged. Five new target checks validate the
selected checker and emitters. The native fixture passes both AST/EIR optimizer
settings with a clean heap (41.84 s), including private access errors and a fresh
receiver whose destructor rewrites the selected property. PHP CLI and native code
agree on FD with no warning: the factory and destructor run before conversion,
and the referenced cell exposes the destructor's replacement value.

The narrower property, dynamic-name/hash, nested erased receiver, Internal
producer and host-boundary audits remain outstanding. This is a staged Casts
integration step, with no public admission change. All 23 focused settype tests,
five existing XML emitter cases, the native XML materialization/ownership control
(66.72 s), check --tests, build and curl exporter pass. Generated docs, site and
EIR architecture audits, assembly comments, example syntax and diff checks pass
with unchanged builtin inventory (828 AOT, 77 graph, 56 primitive, 695 runtime,
679 eval).

### Runtime property-name reference contexts

The shared native/ordinary dynamic-name dispatcher now accepts an ordinary result
adapter as well as its native result adapter. Checked runtime-selected reference
arguments match the evaluated property bytes and reuse the exact-class owned Mixed
cell binder. Literal and dynamic names share one property eligibility predicate
and one initialized-branch merge helper. Receiver/name evaluation and their
existing ownership scopes remain authoritative; names are not evaluated twice.

The extended native regression first failed with an unchanged 10.9 property and
FNDW from a stale pre-destructor snapshot. It now publishes 10, preserves the XML
view, and matches PHP FND without a warning. Additional cases preserve the late
variable receiver selected by the property-name callback (N11.9:12), private-access
errors and allocated name cleanup. Both AST/EIR optimizer settings pass with a
clean heap (46.36 s). The shared fixture is also covered by all five selected-
checker target tests. All 23 settype tests, five existing XML emitter cases and
the NoFlag XML materialization control (67.28 s) pass. Check --tests passed on an
unchanged retry after one integration-check process failed; build and the curl
exporter pass with -j 4. Generated builtin/site/EIR audits, assembly comments,
example syntax and diff checks pass with unchanged inventory (828 AOT, 77 graph,
56 primitive, 695 runtime, 679 eval).

This completes the tested runtime-selected name path for declared owned Mixed
slots. Narrower and hash-backed destinations, nested erased receivers, static
known-callee dynamic-place staging, Internal producers and host/eval admission
remain active work before publishing Casts. No public feature mask changes.

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
| 75 | this commit | feat(strict): preserve runtime property reference names |
