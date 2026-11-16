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

Verified through step107 (this commit), with original-kind admission audits in progress. Issue:
https://github.com/illegalstudio/elephc/issues/2347.

The full task remains active. The user requested complete implementation, with a
commit for every verified step and no partial delivery. Do not expose unfinished
flags as successful no-ops. The public integrated mask still contains only
Locals, Bools, DynamicCode, Opcache, Arithmetic and Overflow. Existing Locals behavior remains available
but its stronger numeric/reference/eval binding contract is outstanding. All
other guarantees and the complete umbrella remain explicitly gated until their
whole contract is enforced.

This progress ledger condenses verified implementation through step107.
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

### Current casts and reference implementation

Checked numeric/string conversion plans, native object-number adapters, original
GMP projections, settype target aliases and collection targets, prepared scalar
builtin graphs and protected contextual warnings are implemented. Current
invocation policy stays distinct from physical Internal producer syntax. Builtin
contract ownership, callback effects and result kinds remain shared across direct,
named, dynamic and first-class calls.

Prepared writable places capture their original binding with LocalPlace markers
at source argument position and read a rooted current-value snapshot after all
arguments finish. Owned instance scalar, nullable, union, object, array and
Iterable cells share canonical promotion and immutable declaration validation.
Generic receiver reloads use the canonical reader; raw eval identity reads retain
their separate ABI. Exact Mixed retains its established metadata flag, while
reviewed constrained payloads use bit10. Projection authenticates resolved shape.
Raw Iterable metadata uses tag13 and a sentinel-safe leaf reader reused by JSON,
serialization and hydration, preserving arrays, hashes and Traversable values.

Boxed static bindings reserve24 bytes and register their physical declaration
once. Layout2 includes canonical promotion and weak-source registration; repeated
selection preserves generation. Retirement detaches metadata before PHP cleanup.
Source references and eval exports share the descriptor selector and wrapper
identity. The versioned eval reference replacement boundary validates through the
same coordinator and transfers the displaced owner back to Rust before snapshot
cleanup can throw, preserving eval metadata-update/release ordering.

Public Arithmetic and Overflow are independently admitted. Their native fusion,
warning, callable, source-fold and eval boundaries retain checked policies and
ownership. The other profiles and the umbrella remain gated as described above.
Detailed historical steps63 through87 remain in their Git commits, earlier plan
revisions and architecture document revision76; the full commit ledger below
retains their traceability. The approved contract and remaining delivery scope
above are unchanged.

### Concrete static storage and live conversion references

- The shared static storage planner keeps original declarations in ClassInfo,
  selecting boxed physical storage for reviewed user statics under Casts. Native
  reads project the declared ABI from that box; validators still use the original
  int, float, bool, string, nullable, object or Iterable contract.
- Assignment and Reflection ownership planning, eval getters/exports, and
  process/web owner enumeration consume the same physical policy. Typed scalar
  statics now own boxes and participate in cleanup just like constrained unions.
- Literal defaults also follow the physical policy in the initializer prologue,
  which bypasses ordinary EIR static stores. They retain their original declared
  conversion before their resulting owned native value is boxed for storage.
- Native fixtures cover inherited aliases, conversion and rejected replacement,
  declaration-preserving scalar results, object/Iterable values, direct Mixed
  reference functions, Reflection, self-assignment, missing fields and GC.
- The physical policy unit, selected-checker/EIR generation and export checks on
  all five targets, three native static bundles with both optimizer settings and
  clean heaps, two permissive Reflection/string lifetime controls, architecture828
  and assembly/diff checks pass. The fixture converts an array and separately
  replaces it with an iterator: pre-existing ArrayIterator array-cast behavior is
  tracked independently as task97375be9.
- Before Casts admission, complete protected direct static setter publication,
  including unbound declaration coercion and initialization visibility during
  callbacks. Concrete checked eval setters currently return a guarded miss rather
  than write through a raw-layout handler. The public profile mask is unchanged.

### Validated static publication and initialization

- Direct checked static stores prepare a canonical binding even before an alias
  exists. Uninitialized bindings retain their sentinel during conversion callbacks;
  an accepted static commit publishes the child and initialization before cleanup.
- Native, deferred-eval and initialized-static publication share the same rooted
  validation body. A typed retirement plan selects old-value cleanup, transfer to
  Rust or static initialization; it does not duplicate declaration coercion.
- Static store effects conservatively include arbitrary PHP conversion and cleanup
  callbacks while excluding invented direct blocking/network I/O boundaries.
- Selected-checker/EIR and effect tests, commit/publisher units, four static cell
  tests including all-five-target real assembly, protected reference wrapper
  assembly, three native static bundles and five publication ownership controls
  pass. Native cases cover both optimizer modes with clean heaps.
- Direct Reflection fixtures verify unbound float coercion, failed writes,
  uninitialized Stringable conversion, contained exceptions and same-field reentry.
  Use fresh known Reflection receivers for this path. After broad catch effects,
  a receiver can enter the runtime fallback, whose protected setter wiring is next.
- Before Casts admission, audit PHP's temporary Reflection Stringable destruction
  order (knowledgec1351278) separately from old-value retirement. Named retained
  source values and explicit error release make the current publication probe's
  lifetimes intentional.

### Protected scoped runtime static setters

- Checked runtime static slots now select the same binding preparation and static
  publisher as direct EIR writes. Their C adapter borrows the original RHS and
  returns a boxed Throwable through an explicit output.
- A versioned scoped setter captures the existing seven arguments plus a Throwable
  output, preserving register/stack ABI placement. It calls the existing scoped
  dispatcher inside native exception containment, then returns miss, success or
  Throwable status to Magician. Legacy native callers keep their boolean entry.
- RuntimeValueOps consumes the explicit status and schedules the returned Throwable
  through ordinary interpreter catches. Scope and visibility still come from the
  original dispatcher; declaration validation remains in the shared coordinator.
- Direct versioned C ABI probes exercise conversion, rejection, initialization,
  callback reentry and private scoped access under staged Casts. A real opaque
  eval constructs its own Reflection receiver to verify the Rust Throwable path.
  Reusing a native Reflection receiver after catch effects is an unreliable proxy:
  a separate permissive baseline reproduces a silent no-op (taskcbfbae83).
- Verified both new native fixtures with both optimizer settings and clean heaps,
  real five-target assembly for the scoped setter and runtime publisher, compiler
  check/build, architecture inventory828, assembly comments and diff hygiene.

### Exact declaration references under Coercions

- A shared checked-property eligibility helper selects canonical declaration
  storage for Casts or Coercions independently. Source references, static storage,
  native metadata and scoped runtime setters consume that same decision.
- Native declaration validators distinguish PHP strict_types widening from exact
  Coercions membership. User declarations under Coercions reject int-to-float
  changes regardless of the PHP caller bit, while unions preserve exact members.
  Trusted implementation declarations retain their reviewed PHP behavior.
- Prepared builtin reference argument graphs are selected independently from cast
  domain checks. Coercions-only settype therefore writes the original checked
  place but preserves permissive explicit conversion domains. Descriptor argument
  selection and call_user_func value semantics use the shared reference policy.
- Checked callable catalogs can materialize settype wrappers after source scans.
  Profile-aware object-cast prelude injection reserves their declared dependencies
  even without an explicit source settype or object cast; permissive usage-based
  injection remains unchanged.
- Staged fixtures cover scalar static/instance aliases, prepared settype, declared
  unions, later direct writes and independent cast behavior. A direct versioned
  static setter fixture covers all four Casts/Coercions combinations and contained
  errors. All five-target selected-checker/EIR checks, both optimizer modes,
  every Casts/Coercions combination, native producer/descriptor controls, compiler
  check/build and the complete generated-builtin workflow pass. No generated docs
  changed. The native module test helper links the exact reference archive from
  Cargo JSON output inside its queued job. Public admission stays at six until
  the remaining boundaries are done.

### Exact eval-owned property values

- Shared runtime membership now lives in value_contracts.rs, retaining scalar,
  nominal, union, intersection, generic and callable rules from return checking.
  Declaration owners resolve self/parent independently of the writing scope.
- A focused property_value_contracts adapter checks candidates before instance,
  static, Reflection or reference-binding publication under Coercions. Existing
  permissive non-null object diagnostics remain in their original policy.
- The bytecode engine deliberately reports PropertySet as a compilation gap.
  Focused VM-call tests enter the ordinary walker function-body fallback, so both
  supported execution routes observe the same property guard.
- Whole-owner checks exposed independent activation leases in the fake Reflection
  factory. Fresh metadata fields now transfer into the fake object graph, and
  class-only flags are allocated only for class reflectors. A permissive control
  checks that ledger independently of strict property behavior.
- Verified scalar/union/structural and lexical nominal cases over direct and
  reflected writes with walker and VM-call fallback, plus a permissive Reflection
  owner control. Existing null property/reference/return, exact parameter/return
  and fake graph-owner controls pass. A real opaque native eval Stringable throw
  passes both optimizer modes with clean heaps. Compiler check/build, architecture
  inventory828 and hygiene pass. Coercions and Casts remain staged.

### Live eval-owned property reference writes

- Checked property bindings promote source variables into persistent reference cells.
  The context projects current instance/static associations into borrowed declaration
  constraints, including parent-private storage names. No association retains a receiver.
- Interpreter cell replacement checks every live property source before publication.
  Alias assignments, by-reference parameters, invoker slots and escaped closure
  captures use this adapter. Unset, rebind and metadata retirement release only the
  departing source, while other sources continue to constrain a shared cell.
- Five new regressions cover assignment-point errors before later effects, escaped
  captures, source retirement, private shadowing and multiple live declarations.
  Walker and supported VM caller/fallback paths preserve the previous value and
  balance all fake runtime owners. The fake retirement case explicitly invokes the
  native metadata-forget equivalent; it does not claim to test native finalization.
- All eight property tests, existing null-property/local-reference/settype/owner
  controls and the real opaque native eval Stringable exception test pass. Compiler
  check/build, architecture inventory828, assembly comments and diff hygiene pass.
  Native stores outside the interpreter and cross-context interoperability remain
  admission work. Coercions and Casts stay staged; the public feature mask is six.

### Eval-owned constraints at native reference publication

- The native coordinator checks eval-owned property sources before its native
  declaration coercers. A versioned Magician callback returns success, failure
  or an owned raw Throwable; no native exception crosses Rust. Inputs stay borrowed.
- The callback copies live thread-local ABI context handles and an active native
  caller, then checks their weak property associations. The interpreter also checks
  foreign live sources and moves a source owner's TypeError to the writing context.
  Context and receiver lifetimes are not extended by these metadata scans.
- EIR requests canonical validation for Coercions with eval even when an opaque
  class has no corresponding AOT property declaration. Other profiles and binaries
  without the eval bridge retain their existing dependency boundary.
- A real opaque native experiment was red with later callee effects and a changed
  integer value. With the guard it passes both optimizers with clean heaps,
  including native catches, nested eval, static/shared sources, unset and native
  object finalization. That experiment temporarily opened the development runtime
  admission gate. The gate was restored before this commit because a separate
  builtin probe still accepts strlen(7) under Coercions. Those integration fixtures
  remain pending the completed runtime admission step and are not this commit's CI
  coverage. No new public profile is enabled.
- Nine property tests, the pending-profile API rejection, three native coordinator
  tests including real five-target clang assembly, compiler check/build,
  architecture inventory828 and selected-source hygiene pass. Next use neutral
  builtin parameter types and existing argument ownership/binding paths to close
  builtin conversions before admitting Coercions; then finish all other profiles.

### Eval builtin value input contracts

- The evaluated builtin dispatcher validates fixed input parameters against the
  canonical neutral TypeSpec contract before runtime or value hooks can convert
  them. Scalars, nullable types, unions, arrays, objects and callable membership
  share one adapter. No second builtin name or signature table was introduced.
- Mixed input APIs and explicit conversion functions retain their declared
  behavior. Null defaults remain explicit API sentinels. Reference outputs retain
  their callee-owned validation and writeback policy, since their old kind can be
  irrelevant to the operation. Special source/direct reference handlers still
  require their own shared boundary before runtime admission.
- A strlen(7) fixture was red with output1 and no exception. Three new tests cover
  direct, named, spread, callback and mixed-case calls; scalar/null rejections;
  exact and explicit input controls; a supported VM caller; Stringable rejection
  before its hook; and complete fake owner accounting. The old one-argument eval
  implode limitation is tracked separately as task94773458; the strict control
  uses its supported two-argument form.
- All fifteen selected parameter, return, property and pending-profile tests pass,
  plus compiler check/build and the curl builtin exporter. The complete generated
  builtin workflow passes with no generated diff, architecture inventory828,
  assembly comments and diff hygiene. Coercions remains staged with six public
  features until special source handlers, AOT/folding and host audits are complete.

### Eval source builtin contracts and reference owners

- Selected eval signature parameter types now travel with EvalParamSpec and the
  passing-mode view, including eval-specific extra parameters. Shared source
  wrappers check the already evaluated inputs before invoking original handlers;
  regex outputs, OpenSSL tags, socket/stream outputs and other source-sensitive
  behavior keep their existing targets and source order. Array mutators share
  their established evaluated reference dispatcher under Coercions.
- Checked reference variables, ordinary mutable eval properties and eval statics
  capture stable cells at their source position. Later argument writes are visible
  without reading a property again; an unset/rebind leaves the captured old cell
  separate from the replacement property. Casts-only alias preparation remains an
  independent admission audit.
- Fresh direct replacements share an owning publication helper. Variable storage
  consumes its lease; reference cells and other borrowed targets receive the value
  before its producer lease is retired. Explicit Owned output writes also honor
  that transfer policy outside plain variables. Source/output failures retire the
  candidate while preserving the original target and pending TypeError.
- Rooted sort/splice builders retire owned key/value reads, generated integer keys
  and partial results. Sort collection also cleans reads on conversion failure;
  scalar float extraction retires its conversion temporary. Positive source
  controls exposed these ownership errors rather than accepting leaked results.
- All twenty-five focused tests pass, including nine parameter tests and
  direct/named/callable input checks, exact mutation/output controls, live and
  detached arguments, typed output rejection, legacy splice/settype and complete
  fake owner ledgers. Compiler check --tests, build and curl exporter pass.
  The full generated-builtin workflow passes without generated changes, with
  architecture inventory828 and zero structural errors. Whitespace and function
  documentation checks pass. The six-bit public admission mask remains unchanged.

### Original builtin input evidence before folding

- The source call validator also resolves neutral builtin signatures when no
  source declaration owns the name. User/extern ambiguity and namespace rules
  retain their existing conservative handling. Named/spread placement reuses the
  shared argument planner rather than matching parameter names again.
- A focused builtin input adapter projects selected AOT parameter types with null
  defaults preserved as API sentinels. Mixed input APIs and output references keep
  their specialized contracts. Declaration and builtin source checks share the
  same original-kind proof and diagnostics.
- The original strlen(7) source-validation test was red. Four focused tests now
  pass: builtin rejection before and after initial folding; explicit/exact/null
  sentinel and trusted-source controls; prior scalar parameter rejections; and
  original-value guarded storage emitted for all five targets in both optimizer
  modes. Compiler check/build, the curl exporter and the full builtin docs workflow
  already cover this source snapshot; generated files remain unchanged.
- This closes the proven scalar source evidence gap. Dynamic builtin inputs still
  need typed checker/EIR guards before coercive ABI preparation. Coercions remains
  staged, alongside the independent Casts lifetime and host audits.

### Converted ReflectionClass argument retirement

- ReflectionClass and ReflectionProperty static writes now carry distinct argument
  retirement policies through EIR. The class setter hands its borrowed caller
  parking slot to the checked publisher; ordinary/property setters keep their
  argument until publication. Both parking operands are validated as registered
  unwind owners, and the backend uses target-aware ABI helpers.
- The class-call publisher roots the converted candidate while retiring the source
  snapshot and clearing the caller's argument owner. PHP destructors therefore see
  the previous property. A successful conversion is still published when this
  retirement throws; only then does the pending Throwable reach the caller. The
  initialization marker follows that publication, including an Error caused by
  reading a previously uninitialized property from the destructor.
- Two native tests pass with both optimizers and clean heaps. Five focused root
  tests also pass, covering EIR policy on every target before/after optimization,
  root/commit protocols and real clang assembly for the complete reference write
  family on all five supported targets. Assembly comments and whitespace pass.
  Three adjacent native setter controls also pass, as do compiler check --tests
  and build, the architecture828 gate, and source documentation hygiene.
- Casts remains staged. PHP probes also distinguish a pre-existing reference from
  one introduced by a destructor: follow up with native rebind fixtures before
  claiming binding identity parity. Runtime/opaque ReflectionClass calls and
  Casts-only eval argument aliases remain independent admission audits. The public
  mask still contains the same six completed guarantees.

### Native builtin input contracts

The native dynamic-input regression exposed five unchecked conversions in the
standard positional builtin path: `strlen` accepted int, bool, null and a
Stringable object, and `str_repeat` accepted a float count. The old result was
`1AX1AX0AXS9AXaaAX|1A|abc`, with balanced owners. The corrected five-case fixture
passes under both optimizer settings and has a clean heap.

The common builtin argument entry now projects fixed value-input types from the
neutral AOT metadata before planning. Names, defaults, by-reference modes and
variadic collector shapes stay with the existing signature. Nullable alternatives
are retained. The common parameter-order pass checks evaluated inputs before
builtin-specific value adaptation. JSON's source preparation also runs that pass
before its detached string conversion. Shared and non-lazy checker validation
reuses each already inferred input type for static strict diagnostics.

The expanded native fixture adds named arguments, unpacking, a first-class
callable, later-argument effects, JSON and runtime-null substring length. It
passes under both optimizer settings with clean heaps. Five focused library
checks pass, including typed source diagnostics, trusted implementation controls
and EIR generation on all five targets with both optimizer settings. The compiler
build and builtin architecture audit pass. Numeric and permissive controls are
checked separately. Coercions remains staged while the remaining
source/prelude/host paths are audited.

### Verified scalar comparison core

Step 100 was implemented in the owned `feat/strict-next` worktree while
`feat/strict-mode` retained the verified step 99 checkpoint. The worktree is based
on `b8f700b0467fc1c14794dda1b9068e9b1c6dfcd4`; no feature admission mask changed.

The dependency-neutral comparison policy distinguishes exact scalar ordering,
runtime guards and rejection. Source validation retains loose equality rejection
and original-kind evidence before folding, including nested literal eval.
The native path uses ordinary EIR predicates and branches: matching scalar kinds
are checked before conversion; strings use bytewise `StrCmp`, including spaceship;
other matching scalar kinds keep the reviewed IEEE/PHP scalar kernel. Operand
snapshots precede later source effects. AST folding, exception summaries and DCE
preserve these boundaries. Native switch matching uses strict equality and keeps
null distinct from integer zero.

Walker and VM binary evaluation share the same scalar policy and bytewise string
operation. Eval parsing rejects loose equality tokens before executing source.
Strict eval switch keeps source-order label effects, live variable subjects and
existing operand-owner ledgers, retiring temporary subjects before the case body.
Nine focused tests pass: source/folding/all-target checks, walker and VM scalar
contracts, parse rejection, live switch subjects, the native heap bundle and the
neutral policy matrix. Both native fixtures pass with both optimizer settings
and clean heaps. The compiler build, builtin architecture audit, example syntax
and whitespace/docblocks pass. The runnable example sorts string labels through
an explicit comparator while retaining leading zeros, verified as `02, 10, 9`.
An existing DCE comparison-guard regression also passed. Array construction stays
in the walker in the VM fixture; its comparison still executes in both engines.
Eval's already unsupported `<>` spelling stays unsupported in permissive mode
and gets the strict equality diagnostic in the checked parser.

Comparisons remains staged. Native value-class comparison domains, comparison
builtin policies, typed switch-label evidence and the remaining literal-eval/host
paths must be completed before admitting the feature.

### Prepared source policies and exact switch boundaries

Step 101 adds one reusable expression source-policy stack to the shared AST
visitor. Prepared producers retain their original operation policy through a
physical internal consumer. Nested statements establish their own lexical policy;
expression exit restores the parent policy. Inverse, sibling and nested closure
fixtures cover both accidental exemptions and accidental restrictions.

Typed switch labels use exact PHP kind overlap, plus existing nominal inheritance
and resource compatibility. Variable subjects are checked after label effects,
matching their execution-time lookup. Open unions and interfaces retain valid
possibilities. Nonvariable strict switch subjects use the existing call-owner
rooting protocol and retire before the selected body, including object and string
temporaries. The optimizer keeps the switch boundary instead of rewriting it to
a loose binary comparison with different capture and retirement timing.

Actual regression evidence: source copies incorrectly passed inside internal
statements; known disjoint typed cases passed checking; the native temporary
fixture printed `DB|SL` and leaked one object and one string. After owner repair,
only AST optimization changed its result to `DDX|SX`, exposing the single-case
rewrite. Verification covers independent AST/EIR optimization settings and heap
balance. All eight focused compiler checks and three native fixtures pass;
the temporary-switch fixture passes all four AST/EIR optimizer combinations with
a clean heap. The compiler build, all-five-target EIR emission, architecture
inventory and source hygiene pass. Public admission remains unchanged.

### Numeric producer proof and prepared result binding

Step 102 reuses the shared source-policy traversal in numeric evidence. Recursive
proof selects each prepared producer's mode and PHP strict-types setting, restores
the caller after either success or a diagnostic, and applies the validated
IntegerToFloat binding to its result before a public consumer checks the value.
Declaration binding remains a consumer boundary with its own physical policy.

The two baseline fixtures failed before this change: all eight combinations of
physical/producer mode for Overflow, Arithmetic, Coercions and Casts were inverted,
and a prepared trusted float result was reported as its unbound integer value.
Both fixtures pass after the fix. Nine focused source/checker tests and the
compiler build pass in the original checkout, including existing declaration,
trusted constant, user/builtin call and all-five-target storage controls. The
baseline failures and the independent successful rerun establish the regression
and its correction without changing public feature admission.

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
| 107 | this commit | fix(strict): reject logical null string operands |


### Builtin comparison policy integration

The canonical builtin contract now carries comparison metadata, keyed by stable
identity and parameter name. Searches
require explicit identity mode. Automatic comparison APIs direct the caller to
a source-selected comparator; enumeration-only `array_keys()` remains valid.
Existing defaults stay unchanged. Compiler source proof consumes the common call
planner; a shared projection handles fixed source/default operands without
assuming values supplied by dynamic unpacking. Native mode checks run after all
source arguments, retain operand owners and emit ordinary EIR. Eval's existing
named/reference binder and positional value boundary share the same policy.
Comparison-mode failures are observable to AST DCE and exceptional flow.

Tests cover static omitted/false modes before folding, named/static spread
arguments, prepared producer modes, all-five-target EIR, native callable/default
mode failures, source effects, heap accounting and eval walker/VM consumers.
The label example now demonstrates an explicit identity membership check.
Focused source, prepared producer, neutral contract and all-target EIR checks
passed. Native descriptor fallback now uses prepared EIR bodies, including a
rejection-only body for automatic comparison APIs. Reused callables after a caught
failure retain explicit/default mode guards and normal return/argument ownership.
Direct eval sort calls also consume the shared evaluated comparison contract,
independently of Coercions. VM tests use compiled callers with the supported
interpreter fallback rather than claiming unsupported direct builtin bytecode.

Actual owner regressions exposed and fixed two separate search issues. Eval now
retires each key, fetched entry and comparison result through the existing owner
finisher. Native array_search returns a fresh boxed key or false, never its
haystack allocation; its previous alias metadata leaked a temporary string array
and both entries (152 bytes), also in permissive mode. New native isolation
controls and the complete builtin failure bundle pass with clean heaps; the
bundle covers every AST/EIR optimizer combination. Four focused eval tests and
all-five-target descriptor checks pass. Compiler build, curl exporter, complete
generated builtin documentation workflow (828 AOT, 679 eval), assembly-comment
alignment and diff hygiene pass. The local Rust1.99 toolchain reports only the
previously documented inherited native_catalog deprecation and test import warnings.

No policy has been newly admitted or claimed complete. Production implementation
and verification use the original checkout and its own Cargo target directory.
Strings source baselines have established 14 rejected cases are still accepted,
with explicit formatting positive controls passing. New, unregistered Strings
helpers and fixtures are prepared for the next step and excluded from step103.


### Verified string compiler and native integration

String concatenation now checks original string kinds through shared source/type
helpers and EIR predicates, independently of Coercions. Formatting of scalars and
Stringable values remains an explicit context; arrays are refused before output.
The compiler lexer emits an internal StringFormat prefix for every interpolated
operand, represented by CastType::StringFormat. Folding, type/result/alias
projections, missing-string handling, source reconstruction and AOT eligibility
retain this context. Generated source uses braced interpolation when possible and
a scoped, once-evaluated formatting argument for other prepared expressions.

Native concatenation evaluates and captures both original operands before
checking them, sharing the existing snapshot and unwind owner helpers. Compound
stores validate before publication. Native output guards retain temporary owners.
A focused regression also exposed print's existing omitted temporary retirement:
print new Stringable left one24-byte object and skipped its destructor. Print now
uses the same owned-value retirement as echo, with a separate permissive control.
Another actual integration red showed a lost Stringable catch under AST
optimization because StringFormat lacked the ordinary String cast throw summary;
that projection is now shared and its strict/permissive native regression passes
all4 AST/EIR optimizer combinations with clean heaps.

The14 source rejection baselines and formatting positives pass. Five source,
producer/consumer, reconstruction and all-five-target EIR tests pass, together
with5 exact lexer tests and1 Pratt/parser provenance regression. Native controls
cover all4 AST/EIR combinations for failures, alias snapshots, compound-reference
writes, output union failures, hooks and owners; the row-label example and
permissive print controls pass. Final focused verification and compiler build
passed after the throw-summary repair, together with architecture828, assembly
comment alignment, example syntax and diff hygiene. Eval and host parity remain
the next step, and Strings remains staged. No new public profile is admitted.


### Verified eval string integration

Both eval consumers now retain StringFormat through lexer tokens, EvalCastType
and the bytecode formatting opcode. Shared checked_strings helpers validate
original concatenation kinds and array formatting before conversion hooks.
Walker echo and owned output retain their separate existing lease protocols;
both call the same formatting check. Explicit casts preserve their own behavior.
Compiler and eval diagnostics share neutral conversion messages.

The actual baseline accepted all14 expected rejection cases across walker and VM.
Those failures now throw TypeError. Four new eval tests cover original kinds,
source effects, explicit conversions, Stringable hooks/throws, compound local,
alias, entry and property stores, and whole fake owner accounting. These and the
existing explicit string conversion control pass. Eleven exact lexer fixtures,
a parser provenance/precedence test and the old textual-key interpolation control
pass. Compiler reconstruction/all-five-target checks, two native owner/throw
bundles and compiler build also pass after the shared eval implementation.

The already-built format guard reproduced the schema mismatch: fingerprint
3110961241602077184 must become14787256250629008100. File-cache format31 now
retains formatting casts and tokens; a new disk roundtrip checks that distinction,
and touched cache fixtures use explicit directory owners. Main's live format was
28 when inspected on2026-11-16. Schema identity, old-version refusal, ordinary
and formatting disk roundtrips, compiler build and real runtime scalar
interpolation compatibility all pass. The runtime scalar harness uses a directory
owner on every exit; its archive is prebuilt so inherited Cargo warnings do not
become compiler diagnostics. No profile is newly admitted.

A separate106 native XML property-update probe reproduced a strict bypass after
both permissive optimizer controls passed. An erased receiver accepted integer7
and stringx through the specialized helper and modified the XML: output
Xa7|Xa7x|a7xx instead of Ta|Ta|ax. The heap remained clean. Its module registration
and new fixture stay outside105. Public original-kind validation before native
write-fetch, further native views and broader host parity remain required before
Strings admission.


### Native string update boundary (106, verified)

The reproduced native XML bypass is addressed by a shared, non-mutating
ObjectProperty::UpdateRead adapter. It preserves empty-name and uninitialized
receiver diagnostics before a readonly property selection. Both EIR and eval
inspect that original result and the already evaluated RHS under Strings before
entering the existing writable update helper. Permissive source, trusted
implementation source, ordinary object fallback and numeric update operations
retain their existing paths. Verification is queued as2195bebd: native named and
dynamic update fixtures plus the independent eval protocol owner control. No
feature admission is changed while those results remain pending.

Job2195bebd reached compilation and found a moved-value error in the new fake
fixture before any execution test ran. The fixture now clones its per-case
source value. Retryd9bd6db8 (session53650) includes the eval owner test, five
first-class target emitter tests with EIR optimization on/off, both native XML
execution fixtures, compiler build and builtin exporter. The all-target module
is registered. The missing typed-string read probe is also registered but stays
outside the retry filter; its concern remains unverified until separately run.
No native fix is yet verified or committed.


### Native106 verification and typed-string107 baseline

Retryd9bd6db8 exited0 after14m47s. The eval native protocol owner test passed;
all five supported target emitter tests passed in both EIR optimization modes
and public/internal source controls; both real XML named/dynamic execution
fixtures passed in strict/permissive and optimized/unoptimized modes with clean
heaps; compiler and curl builtin exporter builds passed. The generated builtin
workflow, module/comparison generation, docs/site audits and architecture audit
all passed with no generated metadata drift. The first all-target test fixture
retained unrelated XML/DOM declarations and took525.27s collectively. It now
uses the production reachability pass with explicit exported probe functions and
asserts that the neutral contract retains UpdateRead. That test-only refinement
is queued as26b68d0b/session56243; it must pass before106 is committed.

Separate baseline25bec80c/session84112 reproduced the typed-string missing-read
issue after four permissive AST/EIR controls passed. Strings with both optimizers
off produced x|ax|[]|x instead of T|ax|[]|x. Formatting, explicit casts and heap
balance passed; the original null kind is lost before the concat check. Fact
6a822db7 captures the evidence. The fixture and its module registration remain
outside106. Strings is still staged and the complete implementation continues.


Final106 fixture verification26b68d0b exited0: all five supported target tests
passed after production reachability pruning, with both EIR optimization modes
and public/internal source controls retained. Collective execution dropped from
525.27s to391.26s. Together with the native/eval, build and generated-doc results
above,106 is verified. The new missing-string fixture and its registration stay
outside this commit. Strings remains staged because107 still needs its logical
null check. Inspection confirms str_persist preserves the dedicated sentinel on
both architectures; TypePredicate(String) alone accepts concrete Str storage,
while IsNull already checks its logical missing value (facte2d23047).


### Logical null behind typed string storage (107, verified)

The concat contract now uses the existing target-aware IsNull operation for
concrete Str storage before the ordinary kind predicate, with the same unwind
owner pin discipline. Ownership/persistence and explicit formatting/casts retain
their existing behavior. The small original five-target string fixture now also
includes a typed string-array miss and asserts that the logical guard survives
EIR optimization in public source and stays absent in trusted implementation
source. Queued060fc446/session97410 selects that exact library test, the
eight-program strict/permissive AST/EIR missing-read regression and compiler
build. The native XML106 fix is already signed and complete; no feature is newly
admitted while107 and later host/boundary audits remain pending.


A separate, currently unregistered tests/codegen/strict_string_boxed_nulls.rs
probe is prepared for the next audit. It distinguishes missing typed-string
values passed through native mixed arguments/returns from genuine empty strings.
PHP syntax validation passed; no execution result exists yet. Register and run
it after107 before claiming Mixed/native boundary or Strings admission parity.


Final107 job060fc446 exited0 after3m22s. The existing small string guard test
passed all5 target/source-policy checks with both EIR optimization modes in0.98s.
The native missing-read regression passed all8 strict/permissive AST/EIR mode
combinations with clean heaps, correct TypeError behavior and unchanged explicit
formatting/cast controls. Compiler build passed. This107 commit excludes the
new boxed-null probe and its module registration. That probe is now registered
and queued as2892c6b5/session8050 for the next boundary audit. No execution result
exists for it yet, and Strings admission remains closed.
