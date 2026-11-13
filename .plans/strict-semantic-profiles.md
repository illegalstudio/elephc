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
`feat/strict-mode`. This document is an implementation proposal, not a claim
that the flags below already exist. Only this plan was added during analysis.

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

## Proposed feature catalog and ownership

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

Analysis validation: no Rust build or test was necessary for this plan-only
change. Implementation verification remains entirely pending.

## Implementation progress

The shared `elephc-semantics` rlib, stable 16-feature catalog, independent request
resolution, and neutral key/numeric/conversion helpers are implemented. The five
profile tests and initial seven scalar/conversion tests pass. Numeric string
casts now use checked decimal accumulation, including exponent forms, rather
than rounding through a double; the extended boundary tests pass.

The public option and maintained compiler/eval identifiers now use `php-only`.
Builtin documentation was regenerated from the curl-enabled exporter. Catalog,
site compatibility, and EIR boundary audits pass without errors. The two focused native PHP-only CLI compatibility tests pass.

Explicit profiles flow through CLI configuration, checking results, EIR,
function/closure lowering, emitters, runtime generation, and runtime-cache
partitioning. Eval contexts accept a versioned immutable profile through a new
native setter. The default profile is pinned on first activation. Magician's
executable and negative-gap caches include semantic, PHP-only, and PHP-version
identity. Six eval activation/FFI/cache/interpreter tests and seven compiler
profile tests pass, including setter emission on all five supported targets.
A shared integration mask rejects execution of incomplete profiles in both the
compiler and eval embedding APIs. The ordinary compiler build and final type
check pass; only the pre-existing Magician test-import and local-Rust deprecation
warnings remain.

The existing exhaustive PHP-only AST audit now uses a shared `AstVisitor`, split
into statement, expression, and member/type traversal leaves. Policy-specific
PHP-only diagnostics remain in their own small adapter. All 40 directly affected
PHP-only AST audit regressions pass. Strict fixtures now use a reusable owned
project helper that cleans its exact directory even after assertion failures.

`strict-dynamic-code` is implemented at source, EIR, eval FFI, interpreter, and
REPL boundaries. Source restrictions run before folding, recursively audit
literal eval, and reject runtime source even in unreachable code. Literal eval
that still requires the interpreter is rejected at lowering. Validation runs at
physical source finalization, with the explicit profile propagated through both
resolver passes and autoloaded/nested includes. All five native strict cases pass,
including physical include and Composer autoload regressions, literal-eval
optimizer parity, nonempty-profile ABI installation, and refused interpreter
fallback. Static includes keep their existing resolver path.

Other new guarantees are still under implementation. Until a guarantee is
complete, both the CLI pipeline and library checker reject it explicitly rather
than produce an unenforced strict binary. The full umbrella is therefore not
available yet. Existing strict-locals behavior remains unchanged and still needs
the stronger stable numeric/reference/eval contracts described above.

String interpolation is currently lowered by the lexer into ordinary concat
tokens. Preserve formatting intent before enforcing string-only concat operands,
so interpolation of a scalar does not become an accidental type violation.

Design issue: https://github.com/illegalstudio/elephc/issues/2347.

Verification summary for this tranche: 72 distinct focused tests passed (12 neutral
contract tests, 7 compiler profile/target tests, 6 eval profile tests, 40 PHP-only
AST audits, and 7 native CLI tests). Builtin docs regeneration and all three audits
passed. Assembly-comment checks passed on all 31 modified backend/runtime files.
The full suite and native execution on other hosts were not run locally. No task
fixture directories or generated Python bytecode remain. The foundation is committed
as `09ab0ce4af`; no PR exists. Each subsequent complete, verified step is committed
separately, as requested by the user.


### Boolean integration

Strict boolean conditions now use a shared kind guard and typed EIR failure
instructions. Known incompatible kinds are source/checker diagnostics; mixed
runtime operands raise TypeError. Conditions, lazy logical operations, xor,
ternaries, predicate callbacks and Iterator results share that rule. Explicit
boolean casts and intentional absence/data-filtering operations keep their
separate contracts. PHP and LFC statements carry their source mode into checking
and lowering, while reviewed internal implementation tests retain their semantics.

The explicit profile also reaches PHP extension runtime generation. A narrowly
scoped optimizer adapter protects strict conditions during folding, propagation,
control pruning and effect/exception analysis, without changing backend profile
ownership. Both Magician engines and callback adapters use the same condition
helper. Predicate runtime guards distinguish borrowed and consumed result cells.

Focused tests already cover source rejection, both eval engines, native EIR
optimization on/off, predicate/FCC result ownership, iterator failure ownership,
and generation on all five targets. An explicit branch between global runtime
helper entries is required for section GC; the initial fallthrough crashed when
the destination section was stripped. The iterator's retained descriptor is now
published to the ordinary exceptional-owner chain and is released on a same-frame
catch. Both predicate and iterator strict failures pass heap-debug cleanup checks.
Native opaque eval, the real cached REPL (including session recovery), and all
builtin-doc generation/audits pass. The REPL bootstrap now compares its integer
input status explicitly. Fourteen new focused tests pass, plus two permissive
iterator compatibility regressions. Throwing callback-result destructors retain
the TypeError as their previous exception and leave a clean heap. All modified
backend assembly comments and Rust file preambles pass their hygiene checks.
No task-owned fixture directories or Python bytecode remain.


### Integer literal evidence

A shared exact unsigned-magnitude classifier now distinguishes ordinary signed
integers, the unsigned magnitude of i64::MIN, overflowing integers and malformed
digits. The compiler and eval lexers expose explicit profile-aware entry points.
Their parsers accept the minimum magnitude only beneath direct unary minus,
respecting power and postfix precedence. Deliberately written floats, including
IEEE infinities, retain floating semantics. Physical entry, include and autoload
lexing receive the immutable profile; complex interpolation inherits it too.
Eval numeric scanning now lives in a cohesive lexer/numbers.rs leaf.

Simple interpolation offsets require a separate key-syntax decision. Reference
PHP classifies canonical numeric text before ArrayAccess dispatch: 1 is an int,
while 01, -0 and an out-of-range numeric spelling remain exact strings. Both
lexers now consume the shared key classifier, fixing the compiler's zero fallback
and eval's rejection of oversized textual offsets. Exact-key policy keeps the
canonical spelling raw as well. Complex interpolation still parses actual integer
expressions and therefore observes checked literal bounds.

Ten focused tests pass across the neutral policy, both frontend parsers, native
array/ArrayAccess output and eval output. Profile-dependent parsed-source caches,
literal eval plans, runtime arithmetic and checked conversion integration remain
pending. Overflow and other unfinished flags stay explicitly gated until their
entire guarantee is implemented. This is supporting frontend work, not completion
of the strict-overflow feature.


### Parsed-source cache profile step

Eval fragments and uncached include memos now partition positive and negative
parse results by immutable semantic profile with one total FIFO capacity. Eval
execution, scope scans, deferred native-catalog checks and includes pass the
context profile explicitly. OPcache keeps one slot per canonical path in memory
and on disk. A different parser profile is a miss regardless of timestamp
settings; a failed strict replacement discards the former memory entry. Disk
headers require the neutral semantic ABI and exact profile, and serialized cache
format 29 also accounts for the new checked-integer token and parse error. Native
OPcache path operations use a typed backend operation and pass hidden ABI/profile
arguments through target-aware register helpers. Legacy host symbols retain an
explicit permissive default. Profile-aware AOT fragment metadata is the next step.

Validation: seven focused tests passed, covering cached errors and shared FIFO
capacity, disabled/enabled script caches, profile and ABI disk headers, native
FFI refusals, hidden arguments on every supported target, the serialized format
guard, and native/eval OPcache parity with a runtime-created include. Compiler
builds, builtin docs regeneration/audits and assembly-comment checks passed.


### AOT and pre-parsed source provenance step

Literal eval planners receive the composite immutable EvalSourceProfile, joining
lexical PHP-only visibility with the compilation's semantic guarantees. Source-
sensitive EIR data retains that profile and module validation rejects incompatible
metadata. Internal fragment names include semantic ABI and profile bits. The
compact checked u32 representation preserves Instruction's existing 112-byte
stack budget; native and disk boundaries retain the full neutral u64 encoding.
Checked literal parsing now applies to AOT fragments and raw recursive literal
eval validation before folding. Dependency-only scans explicitly use conservative
permissive parsing; actual lowering, liveness and codegen use the attached profile.

EvalProgram retains parser-profile provenance, including through serialization.
A shared execution guard validates context compatibility and supported guarantees
before either bytecode or walker execution, including direct included-file replay.
Native deferred-default expression parsing inherits its context profile. Disk
cache headers must also agree with the decoded AST's profile. Format 30 records
this new EvalProgram shape. The hostile-length regression includes the new header
fields and owns its scratch directory through unwinding.

Validation: 17 focused tests passed, including three AOT profile tests (all five
targets), direct host/walker provenance refusal, existing boolean and dynamic-code
engine cases, cache identity/format/hostile-length guards, native literal eval in
both optimization modes, opaque eval, runtime-created cached includes, and PHP/LFC
visibility isolation. The compiler build, builtin exporter/docs audits and five
assembly-comment checks passed. No generated builtin documentation changed.


### Native value-hook semantic identity step

Before numeric helpers acquire profile-dependent behavior, native runtime value
hooks must advertise their compiled policy. Two target-aware C getters expose
the neutral semantic ABI and exact u64 profile. RuntimeValueOps reports a fixed
profile, defaulting existing host implementations to permissive. Explicitly
policy-independent adapters may opt out. Shared eval entry validation rejects
mismatched hooks before any value operation, and native context installation
rejects mismatches before pinning the context. A shared versioned decoder owns
ABI/unknown-bit rejection for setters, OPcache calls, native hooks and disk cache
headers. Unsupported-profile staging remains in force.

Validation: eight focused tests passed: versioned decoding, fixed-hook admission,
three cache boundary cases, getter emission on every supported target, a real
native setter rejection followed by successful matching installation, and PHP/LFC
eval visibility compatibility. The compiler build and assembly-comment checks
passed. The native handshake fixture confirms a rejected profile does not pin
or contaminate an otherwise unused context.


### Checked scalar arithmetic core

A compact neutral ArithmeticPolicy resolves Overflow, Arithmetic and Coercions
independently. The compiler adapter selects permissive operations for trusted
Internal statements while retaining the immutable module profile. User integer
addition, subtraction, multiplication and unary negation now lower through a
reusable scalar kernel: wrapping operation, IOverflows and a typed ArithmeticError
failure edge before the result escapes. The shared guard builder supports a
failure predicate directly, preserving the existing all-target overflow-branch
fusion without an extra boolean negation.

The public Overflow feature remains staged. Boxed operations, powers, updates,
compound/reference stores, builtin boundaries, source diagnostics, optimizer
effects and eval parity must be completed before accepting the flag.

Validation: four focused tests passed after recovery from the daemon restart:
independent neutral policy bits, PHP/LFC scalar guards and emission on every
supported target, trusted internal PHP operations, and existing boolean guards.
Compiler build, diff hygiene, module preambles and writing checks passed.

### Checked boxed arithmetic and power core

EIR numeric operations now carry an explicit compact ArithmeticPolicy immediate.
The validator checks its required module profile, throwing effects and boxed
operand representation. Legacy PHP frame-cell and chain optimizations refuse
these checked operations. Lowering creates any needed Mixed boxes explicitly
and publishes their unwind roots before invoking a throwing arithmetic helper.
User and trusted PHP helper entries coexist in one runtime, with the selected
overflow policy saved across operand conversion and square-and-multiply calls.

Checked integer powers and genuine boxed integer powers throw ArithmeticError
instead of promoting on overflow. Overflow alone continues to permit negative
integer exponents with their existing float result and ordinary float powers.
The x86 jump-only failure entry supplies the alignment expected by the shared
exception allocation sequence. Every target uses explicit failure and shared
body edges so native section collection retains the necessary helper code.

Validation: two focused EIR/runtime tests passed across all five targets,
including optimized and unoptimized module checks and profile mismatch
rejection. A native fixture passed in both optimization modes under heap debug,
covering add/sub/mul, scalar and boxed power, signed minimum, negative exponents,
mixed scalar operands and nested owned temporaries. Both runs report a clean
heap. The public Overflow option remains staged while the remaining source,
update, builtin and eval boundaries are implemented.

### Checked arithmetic through AST optimization

The AST optimizer keeps its immutable semantic profile separate from a scoped
statement source mode. Folding, propagation, pruning, DCE, effect analysis and
exception summaries now restore that source mode on every nested return. An
Internal wrapper therefore keeps PHP implementation semantics without erasing
the strict policy of a nested user statement.

Checked addition, subtraction, multiplication, power and unary negation no longer
fold an overflowing integer to a float. The effect model retains potential
ArithmeticError even when an expression result is discarded. Callable exception
summaries carry the same failure so DCE preserves the matching catch clauses.

Validation: the focused folding regression passes for PHP, LFC, Internal and a
mixed nested wrapper. The native arithmetic/ownership fixture now runs both AST
and EIR optimizers when enabled, including unused addition and negation results
inside catches. Both optimization settings pass and report a clean heap.

### Source-known integer overflow

Source validation now rejects proven integer overflow before folding can erase
dead branches or declaration expressions. Its numeric evidence keeps exact i64
intermediates, distinguishes float results and delegates checked operations to
the neutral arithmetic kernel. Unconditional global aliases and direct lexical
class constants support forward references; duplicate declarations, cycles and
unresolved namespace names remain unknown. Resolved `use const` aliases use the
canonical names supplied by the existing name resolver.

Each physical source tree and parsed literal eval fragment owns its own memoized
evidence. Trusted implementation constants keep their PHP result kind when user
code consumes them. The shared source visitor covers defaults, attributes,
closures and unreachable branches without duplicating those traversal rules.

Validation: four focused tests passed, covering overflow and valid boundaries,
numeric strings, floats, negative powers, aliases, cycles, source modes, literal
eval and declaration expression positions. The additional resolved namespace
and imported-alias cases pass. The pre-existing eager predefined-constant token
binding issue is tracked separately as Code Journal task d0932767; this adapter
does not infer a replacement value for names the parser has already erased.

### Checked updates and storage

Integer local/reference updates and typed integer buffer steps now use the shared
checked scalar kernel before storing. Boxed/string updates carry NumericUpdate
policy metadata with throwing effects; legacy PHP counter specializations refuse
that metadata. Explicit operand roots and separately acquired postfix snapshots
remain published through the update and store so an exception cannot lose them.

The strict boxed runtime entry checks genuine integer payloads before the PHP
helper can mutate a unique cell. String payloads share the existing exact numeric
scanner and integer parser through a separate checked entry. Only overflow of an
actual integer step throws: out-of-range numeric strings retain their PHP float
classification, alphabetic strings retain carry semantics, and float updates
retain their ordinary behavior. Trusted Internal updates still select PHP
entries. The overflow transfer uses a reusable wide conditional ABI helper.

Validation: two focused EIR/runtime tests pass across all five targets; the
lowering test also covers PHP/LFC/Internal statements with EIR optimization on
and off. A native fixture passes with AST/EIR optimizers enabled and disabled,
covering prefix/postfix updates, mixed and typed references, numeric strings,
properties, array elements, compound stores and typed buffers. Failed operations
leave values unchanged, successful updates preserve their expected results, and
both runs report a clean heap. Compiler build, assembly-comment alignment and
diff hygiene pass. Builtin numeric contracts and eval parity remain next.


### Builtin invocation profile propagation

Builtin semantic inputs, checker hooks and normalized EIR calls now retain the
immutable semantic profile and an explicit invocation source contract. Numeric
policies remain available to shared semantic hooks. Profile-aware signature entry points
cover checking, EIR, reflection and generated callable descriptors; legacy audit
entry points select permissive PHP semantics explicitly.

Public builtin callables use the user contract even when invoked by an Internal
statement. Their argument expressions still use the physical statement mode.
Known callable inference, result resolution, exceptional operand rooting and
packed argument effects share this separation; lowering never temporarily changes
the source policy while evaluating arguments. Generated wrappers copy the module
profile instead of silently defaulting to permissive semantics.

Validation: the two focused profile tests pass, including direct, first-class,
call_user_func and call_user_func_array paths on all five targets and all three
source modes. Internal-only argument conditions verify that callable contract
selection does not leak into expression checking. Architecture and assembly
comment audits pass. Compiler and docs-exporter builds pass, and the generated
builtin docs audits pass with no generated changes. Subsequent compiler builds
also confirm the cleanup of unused convenience methods. Numeric operations are
integrated in the following steps.


### Checked absolute values and typed builtin operation policies

Numeric runtime calls now carry an explicit ArithmeticPolicy beside the existing
RuntimeFnId, with module identity and throwing-result validation. The ordinary
Function target remains the PHP entry, so legacy integer-sink specializations
cannot erase the new contract. Shared identity readers support dependency,
resource, export and eval metadata analysis without reconstructing a call and
losing its invocation policy.

The abs home declares shared effects and EIR result normalization. Checked
integer inputs retain Int contracts; dynamic inputs preserve runtime int/float
kinds. The backend reuses borrowed stack-cell materialization instead of allocating
untracked argument boxes. A separate checked runtime entry rejects INT_MIN with
ArithmeticError, classifies full numeric strings without float rounding, retains
float kinds and rejects unsupported numeric parameters with TypeError. Trusted
Internal operations keep the ordinary PHP-promoting entry.

Source evidence also covers abs through its typed runtime inventory and shared
argument planner, including named arguments and static positional spreads. It
retains numeric-string completeness, bool/null provenance and PHP strict_types
state, so a parameter error cannot be mistaken for a proven overflow. Normalized
argument clones use fresh address memos with the active constant cycle guard.
Forward or conditional source/extern declarations prevent guessing a builtin
binding, and unresolved namespaces remain conservative.

Validation so far: checker, EIR and runtime tests pass on all five targets and all
source modes. The source-constant regression and four prior numeric-evidence
regressions pass. A native fixture passes with AST/EIR optimization enabled and
disabled and a clean heap across direct, typed, first-class, CUF/CUFA and dynamic
calls, fresh string inputs, exceptions/finally and numeric kind controls. Six
ordinary PHP abs regressions pass. Compiler and docs-exporter builds, full builtin
docs generation/audits, assembly comment alignment and diff hygiene pass. The
final shared-identity refactor passes the same four new unit tests, four prior
constant-evidence tests, both native optimization modes and the compiler build.
Overflow remains staged until power, reductions, conversions, storage contracts
and eval parity are complete.


### Checked builtin power and prepared callable contracts

The pow home now reuses numeric operator graphs under checked overflow, including
GMP dispatch and integer-versus-float result selection. Already evaluated SSA
operands receive explicit Borrow views; fresh intermediate boxes keep independent
ownership. The original Mixed parameter contract already preserves numeric kinds,
so no new argument strategy or signature projection is needed.

A Prepared callable policy routes graph-dependent builtins through the full EIR
lowering context. Generated function metadata marks public builtin invocations
separately from the physical source mode of nested implementation statements.
This preserves checked callable results without imposing user syntax rules on
argument evaluation or generated guards.

Validation: two power units and two profile propagation units pass, including
all five target emitters and all source modes. The native power fixture passes
with both optimizer settings and a clean heap across direct, typed, first-class,
CUF/CUFA and runtime-selected calls. Compiler/exporter builds and the complete generated docs/architecture audits
pass, as do the focused permissive PHP scalar and GMP controls.
Numeric-string power normalization and pre-fold source proofs follow
before exposing the complete overflow guarantee.


### Exact numeric power normalization and source evidence

Checked boxed powers classify both operands through a reusable bounded numeric
parser adapter before dispatching warning callbacks. Integer-form strings, bools
and null keep integer power and overflow checks; floating-form and out-of-range
strings retain float kinds. Invalid kinds raise TypeError. Warning handlers cannot
replace an already captured operand. PHP entries remain separate.

A shared numeric result helper unboxes and releases implementation cells only
when the checker promises an Int or Float contract. Abs and checked pow consume
the same helper. AST effects, throw summaries, global-fact invalidation and EIR
effects preserve warning callbacks and discarded checked powers. Source evidence
now covers pow through its typed inventory and common named/spread planner. Its
Mixed parameters retain operator conversion rules even with PHP strict_types.

Four focused units pass, including all targets and source modes, constant proofs
and typed string/float result normalization. Both native power fixtures pass with
AST/EIR optimization enabled and disabled, exact result kinds, source mutation
from a warning callback, thrown callbacks, discarded results and clean heaps.
Compiler/exporter builds and the complete docs/architecture audits pass. Four
abs units, its native fixture and the earlier boxed arithmetic/power fixture
also pass after sharing the numeric result helper. The pre-existing permissive typed-string
pow backend gap is tracked separately as journal task 85c52504.


### Checked numeric array reductions and guarded map fallbacks

Array sum/product homes now emit typed NumericFunction policies through a shared
EIR descriptor. Separate checked runtime entries reuse numeric classification and
resumable cleanup, with checked add/multiply and no PHP-promoting packed sum fast
path. Callable refinements follow DynamicRuntime metadata, and activation-owned
aggregate operands remain rooted independently of the lowering representation.
Unchecked reduce/map-sum fusion declines checked numeric contracts; the normal
callback path then preserves the checked operation and its failure order.

The newly exercised untyped map fallback revealed a pre-existing Mixed map leak
on callback exceptions. Scalar and Mixed maps now share one guarded emitter. The
Mixed result keeps its descriptor environment ABI and element stamp, and each
transferred owner advances the destination length before the next callback.
Exceptional cleanup therefore retires the initialized prefix, including owned
Mixed cells and strings. Shared callback effects no longer claim direct network
or blocking I/O operations.

Validation: three new runtime/EIR units pass on all five targets and all source
modes. Native fixtures pass both optimizer settings and clean-heap checks across
direct, FCC, CUF/CUFA, dynamic, numeric-string, COW and callback-fusion paths. A
default-profile regression verifies partial Mixed map cleanup after a second-item
PHP exception for scalar and string source widths. Ten existing boxed aggregate
controls, three map controls, the original aggregate runtime unit and both prior
checked power native fixtures pass. Compiler/exporter builds, complete generated
builtin docs and architecture audits, and assembly/diff hygiene pass. Source
evidence for reductions and remaining numeric/storage/eval integration follow
before exposing the overflow flag.


### Dense reduction evidence and integer division boundaries

Source evidence now evaluates dense array_sum/product operands in order, including
forward global/class constant aliases, empty identities and intermediate overflow
before a later float. Unknown keyed/spread layouts remain conservative until the
shared literal-key planner is integrated; they are still checked at runtime.
Intdiv evidence respects strict PHP integer parameters, weak conversion range,
full numeric strings and invalid-call precedence. Proof contexts retain the full
semantic profile, so strict-arithmetic division keeps Float rather than creating
a false overflowing integer intermediate. Cohesive proof tests moved to their
own module.

Integer division reuses the shared target-aware ArithmeticError emitter instead
of duplicating Throwable allocation and unwind assembly. Its existing typed
wrapper now also participates in runtime-selected callable dispatch; this fixes
the accepted dynamic call reaching an undefined-function fatal.

Validation: seven source-evidence tests and all-five-target/source-mode division
emission pass. The native division fixture passes permissive and checked profiles
with both optimizer settings and clean heaps across direct, FCC, CUF/CUFA, dynamic,
overflow, zero-divisor and valid-boundary cases. Two existing PHP intdiv overflow
regressions pass. Compiler/exporter builds, generated builtin docs/architecture
audits, assembly comments and diff hygiene pass. The final dynamic-target unit
and native control rerun also pass on the final sources. Keyed literal evidence,
eval arithmetic and remaining strict guarantees continue in the following steps.


### Typed literal keys and preserved append provenance

A shared literal-array planner retains integer/string identity, original string
bytes, explicit key expressions, all value evaluations and insertion order.
Append history follows the selected PHP version and exact-key profile. Numeric
source evidence uses that plan for keyed/mixed reductions, applies last-wins
values at the original insertion position and keeps unknown spreads conservative.
The parser preserves trailing implicit entries until semantic planning instead
of stamping PHP keys before a profile exists. Default folding retains explicit
conversion expressions and overwritten value effects; fractional float keys now
advance the append cursor correctly.

Validation: two focused planner/proof units, all eight numeric source-evidence
controls, the parser provenance regression and a native default-key/value-effect
fixture pass. Both leading-positional parser controls, the compiler build and
diff hygiene pass. The native fixture reports a clean heap. Exact-key runtime and
producer integration remains staged; the public feature mask is unchanged.


### Explicit checked eval numeric callbacks

Magician user binary/unary arithmetic and public abs/pow dispatch now select a
versioned checked callback explicitly. PHP implementation hooks stay permissive.
The callback borrows its inputs, publishes one independent result on success and
contains PHP exceptions below Rust through the shared full-handler boundary.
Unsupported selectors/runtimes return status one without touching output. Native
class arithmetic keeps the existing reviewed object contracts. Eval fake adapters
share the host numeric-string grammar with compiler proofs and use the neutral
checked integer evaluator.

Focused all-five-target boundary emission, all three shared grammar tests, two
walker/VM/builtin owner matrices (including FCC, CUF/CUFA and case-insensitive
names), permissive abs/pow controls, compiler/exporter builds, generated builtin
docs and architecture audits pass. The native callback matrix passes both profiles
and both optimizer modes with clean heaps, intact failure output and owned Throwable
retirement. The compiler binding-grammar control and assembly/diff hygiene also pass.
The initial fixture used an incorrect runtime feature field, a PHP float-form minimum
literal and an unversioned C import; all were corrected before this verified step.
The public integrated profile mask remains unchanged until numeric/storage/eval parity is
complete. Remaining checked reductions, updates and general arithmetic operand
normalization follow alongside the other strict guarantees.


### Shared exact operands for checked scalar arithmetic

Checked Add/Sub/Mul now share the power operand classifier, numeric snapshots,
warning dispatch and result boxing in strict_mixed_numeric. PHP-promoting helper
entries retain their original kernels; the obsolete checked selector bit inside
those PHP kernels is removed. Checked integer operations retain hardware overflow
guards on both architectures. Invalid numeric kinds throw TypeError; numeric
prefixes invoke their warning handler after both operand payloads are captured.
AST effects and exception-flow summaries cover these conversion callbacks even
when a float operand excludes integer overflow, preserving discarded operations,
global invalidation and catch clauses.

A new native regression failed on872d1c3749 with missing TypeError and warning
markers, then passes both optimizer modes with a clean heap. The new all-five-
target graph unit and existing power unit pass. Existing boxed arithmetic/power,
checked array reductions and callback fusions, numeric-string powers and native
eval callback status/ownership matrices also pass. The compiler build, assembly
comment checks and diff hygiene pass. No builtin contract or documentation
metadata changed in this step. Remaining eval reductions/updates and other strict
features continue before exposing the complete profile family.


### Checked eval array reductions

The versioned numeric callback now appends ArraySum/ArrayProduct selectors and
reuses the checked native aggregate kernels. Eval direct, first-class, dynamic,
CUF/CUFA and case-insensitive routes select the same explicit operation. Direct
source evaluation uses the shared operand-owner boundary instead of leaving a
fresh input array unretired. The fake callback evaluates numeric payloads without
unrooted intermediate cells and retires optional internal-class numeric adapters.

Validation: the focused interpreter matrix covers identities, exact integer
strings, float continuation and overflowing reductions across call routes with
balanced owners. The native callback fixture passes both optimizer modes with
clean heaps, unchanged failure output and retired partial carries/Throwable boxes.
The all-five-target boundary unit, permissive aggregate control and preceding
numeric interpreter matrices pass. Compiler/exporter builds, generated builtin
docs/architecture audits and assembly/diff hygiene pass; generated docs are
unchanged. The older permissive interpreter loop's intermediate-owner cleanup is
tracked separately as journal task b388f531. Checked eval updates and the remaining
strict guarantees continue before public activation.


### Checked eval updates and borrowed native inputs

The numeric callback appends Increment/Decrement selectors. Each selected update
canonicalizes its borrowed input into the protected callback's headerless spills
before invoking the existing update kernel, preventing its unique-cell fast path
from mutating the caller's original value. The output pointer keeps its separate
spill, and no extra retained input can be stranded by a native throw. Null
-decrement warnings execute inside the protected callback. Internal-class update
arithmetic retains the reviewed object adapters. Invalid scalar update domains
throw through the shared TypeError path before replacement.

Walker/VM, member/static and reference consumers pass their context through the
same checked operation selector. Checked update EIR effects and AST exception/
mutation summaries include arbitrary callbacks, retaining catch clauses and
invalidating affected facts. The existing PHP boolean update kind deviation is
tracked in the pre-existing task03217601; duplicate audit task afea61a3 was
explicitly dismissed in favor of that original.

Validation: both focused eval matrices pass for numeric/string domains, empty
strings, null warnings, overflow, original-value preservation and by-ref cleanup.
Both native fixtures pass both optimizer settings with clean heaps, independent
results from uniquely owned inputs, intact failure outputs, protected warning
exceptions and retained AOT catch clauses. The all-five-target callback/update
units, existing AOT storage/update matrix and prior native arithmetic/reduction
callback matrices pass. Compiler build, assembly comments and diff hygiene pass.
Updates use the shared stack-pointer address helper through a crate-local ABI
re-export. No builtin surface or generated documentation metadata changed. Other
strict guarantees and remaining boundary audits continue before activation.
