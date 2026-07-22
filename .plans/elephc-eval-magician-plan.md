# Plan: eval, elephc-magician, and Literal Eval AOT

## Task

- [x] Define the target semantics of `eval`: visible caller scope, persistent
  writes, variables created inside eval visible after eval, `unset`, output,
  parse errors, fragment-local `return`, dynamic declarations, and `$this`.
- [x] Add `crates/elephc-magician` as an optional bridge and link it only when a
  program requires the runtime eval fallback.
- [x] Add the ABI, `RuntimeFeatures`, linker bridge, and runtime helpers needed
  to call `__elephc_eval_execute` from the current EIR backend.
- [x] Implement `ElephcEvalContext` and `ElephcEvalScope` shared by native code
  and the interpreter, including flush/reload of observable locals.
- [x] Implement runtime parsing, EvalIR/interpreter, and the value bridge for the
  eval subset supported by magician.
- [x] Support variables, assignments, output, return, control flow, arrays,
  include/require, dynamic calls, declarations, classes/objects, reflection,
  callables, references/by-ref, and error cleanup in the magician fallback for
  the subset covered by tests.
- [x] Model `eval` as an effect barrier for the optimizer/type checker: no DCE,
  no constant propagation through observable locals, and dynamic fallback where
  needed.
- [x] Add repeatable magician benchmarks with Elephc native, Elephc eval, PHP
  native, and PHP eval variants.
- [x] Add parse cache, parse-error cache, and include parse/file cache without
  freezing context, scope, magic constants, or include_once state.
- [x] Add caches for eval symbol lookup, direct builtin dispatch, callable
  resolution, and conservative `RuntimeValueOps` optimizations.
- [x] Add an unboxed scalar fast path, optional linear EvalIR/stack VM, and
  targeted array/reference/COW optimizations in the bridge.
- [x] Implement conservative literal `eval` AOT for scalars, output, return,
  store/scope read-write, and AOT/fallback assembly markers.
- [x] Extend literal eval AOT to internal locals, `while`, `if`, `break`,
  `continue`, comparisons/truthiness, modulo, and the prime-sum benchmark up to
  `100000`.
- [x] Extend literal eval AOT to common static builtins, known static functions,
  typed public static methods, and static callbacks through `call_user_func*()`.
- [x] Avoid linking `elephc_magician` for programs whose literal eval calls are
  fully AOT.
- [x] Update parity tests to distinguish shared builtins, documented eval-only
  builtins, and static-only builtins not yet present in magician.
- [x] Reduce the remaining manual AOT mini-codegen and converge on internal EIR
  functions for supported literal fragments.
- [ ] Add a shared PHP-fragment grammar corpus that must parse consistently on
  the main compiler frontend and on magician (and stay aligned with AOT
  acceptance where the fragment is a compile-time literal).
- [ ] Document allowed grammar divergences between main and magician (no `<?php`
  in eval fragments, no elephc-only extensions in magician, different
  error/recovery models) and treat any other pure-PHP disagreement as a bug.
- [ ] Add a fix policy: PHP-visible syntax fixes on main that can appear in eval
  must either update magician in the same change or land with an explicit
  divergence note plus a corpus case.
- [ ] Consider a thin pure-PHP `elephc-php-syntax` crate only if dual-parser
  maintenance cost remains high after stable magician fallback lands on main.
  This is opportunistic and is not a merge prerequisite.
- [ ] Define dynamic-name semantics and fallback boundaries for variable
  variables, dynamic instance properties, dynamic static properties, dynamic
  member access, and `isset`/`empty`/`unset` over those forms.
- [ ] Complete or audit magician fallback support for `$$name`, `${$expr}`,
  `$object->$property`, `$object->{$expr}`, nullsafe dynamic property reads,
  dynamic static properties, and their write/ref-like variants.
- [ ] Add conservative AOT classification for statically known dynamic names
  only when variable, object, method, property, and access-context facts are
  precise.
- [ ] Add access-context-aware object/member lowering so specialized
  private/protected reads preserve the original method/class scope instead of
  becoming public call-site property reads.
- [ ] Add call-site method specialization for constant arguments such as
  `$car->getProperty("color")`, guarded by exact receiver/method resolution and
  semantic equivalence tests.
- [ ] Expand AOT only where semantics are covered. Full arrays/iterables,
  references/by-ref, `global`, `static`, unresolved dynamic names,
  `try`/`throw`, include/require, and declarations stay fallback until they have
  a dedicated model and tests.
- [ ] Close or explicitly maintain the static-only builtin gap: implement them
  in magician or keep them in a tested allowlist until eval exposes them.
- [ ] Promote the most useful AOT acceptance benchmarks into the permanent
  benchmark suite without including compile/link time in runtime numbers.
- [ ] Update user/internal docs after every semantic extension of the eval or AOT
  subset.
- [ ] Run focused checks on all three supported targets for every change that
  touches ABI, runtime ownership, eval codegen, or fallback/AOT selection.

## Plan Scope

This plan replaces and merges:

- `.plans/elephc-eval-complete-plan.md`
- `.plans/elephc-eval-aot-complete-plan.md`
- `.plans/elephc-magician-performance-plan.md`

This plan remains in `.plans` to track only the remaining eval/magician work.
All plans in `.plans` must be written in English. Completed sections document
the state already reached and act as guardrails against reintroducing old
approaches or regressions.

## Current State

Eval support has two paths:

1. Runtime fallback through `libelephc-magician`, called by
   `__elephc_eval_execute`.
2. Literal eval AOT, when the fragment is a compile-time-known string and the
   classifier considers it semantically safe.

These paths intentionally use different frontends today:

- literal AOT reuses the **compiler** lexer/parser (`src/lexer/`, `src/parser/`)
  and then classifies the resulting AST;
- runtime fallback uses the **magician** lexer/parser
  (`crates/elephc-magician/src/lexer/`, `crates/elephc-magician/src/parser/`)
  and lowers into EvalIR for the interpreter.

They must not silently disagree on pure PHP fragment validity. Magician remains
a separate staticlib and must not depend on the full `elephc` crate or ship the
compiler into user binaries.

After the rebase onto `main`, the active backend is the EIR path under
`src/ir_lower/`, `src/ir_passes/`, and `src/codegen/lower_inst/`. Historical
references to `src/codegen_ir/` in older plans are obsolete.

Current central files:

- `crates/elephc-magician/src/`
- `src/eval_aot.rs`
- `src/ir_lower/expr/mod.rs`
- `src/ir_lower/program.rs`
- `src/codegen/lower_inst/builtins/eval.rs`
- `src/codegen_support/runtime/eval_bridge.rs`
- `src/codegen_support/runtime_features.rs`
- `tests/codegen/eval.rs`
- `tests/codegen/eval_callables.rs`
- `tests/codegen/eval_callable_ref_errors.rs`
- `tests/codegen/eval_constructors.rs`
- `tests/codegen/eval_closures.rs`
- `tests/codegen/eval_reflection_invocation.rs`
- `tests/builtin_parity_tests.rs`

## Consolidated Architecture

### Magician Fallback

`elephc-magician` is an optional bridge staticlib. Programs without runtime eval
must not link it. The fallback remains mandatory for:

- dynamic eval;
- literal eval that cannot be parsed or is not supported by the AOT classifier;
- constructs whose runtime semantics are not yet modeled in AOT;
- dynamic declarations, include/require, references/by-ref, global/static,
  variable variables, dynamic objects/members, and throwables until covered.

The fallback receives:

- global eval context;
- local eval scope;
- global scope when needed;
- code pointer/length;
- result buffer.

The value model must not diverge from native runtime behavior. Boxing, refcount,
COW, references, and cleanup must stay consistent with the elephc runtime.

### Scope Sync

Native code must synchronize with eval scope only for values observable by the
fragment:

- before the call: flush variables read or written when needed;
- during eval: magician operates on the shared scope;
- after eval: reload variables that may have been written, created, or unset.

When analysis is imprecise, semantics wins over performance: use the fallback or
treat the fragment as a stronger barrier.

### Literal Eval AOT

The compiler analyzes literal fragments at compile time:

```text
literal string
  -> parse as PHP fragment (compiler frontend)
  -> normalize/name-resolve compatibly with the context
  -> classify AOT eligibility
  -> plan reads/writes/calls/fallback
  -> native lowering or magician fallback
```

The AOT plan must preserve:

- `return expr;` returns from eval, not from the caller;
- fallthrough without `return` produces `null`;
- output remains a visible side effect;
- caller variables known at compile time can be read and written;
- variables created by the fragment are visible after eval if that AOT path
  declares creation support;
- every uncovered construct remains an explicit fallback.

AOT paths emit assembly markers such as `eval literal AOT compiled...`.
Fallback paths emit markers with a readable reason where possible.

### Dual frontends (compiler vs magician)

Two frontends are an intentional architectural trade-off, not an accident to
erase before landing:

- magician links into user programs and must stay a small optional bridge;
- magician emits EvalIR for by-name runtime execution, not the compile AST that
  feeds type checking, optimization, and EIR lowering;
- the compiler frontend also accepts elephc-only extensions (`ptr`, `buffer`,
  `extern`, `packed class`, `ifdef`, …) that eval fragments must reject.

Near-term governance is corpus + documented divergences + fix policy. A shared
pure-PHP syntax crate is a later option only if maintenance cost justifies it.
Do not merge the full compiler parser into magician, and do not make magician
depend on the main `elephc` crate, as a prerequisite for landing eval.

## Completed Work

### Eval Runtime and Bridge

Completed:

- `elephc-magician` crate;
- C/Rust ABI for `__elephc_eval_execute`;
- `elephc_magician` linker bridge;
- runtime feature detection;
- eval language construct in checking/lowering;
- materialized scope, context, and value bridge;
- observable-local flush/reload;
- error/status mapping and cleanup.

Codegen and interpreter coverage includes eval at top level, in functions, and
in methods, shared scope, nested eval, return/output, created variables, local
mutation, callables, constructors, closures, and reflection.

### Magician Interpreter

Completed for the current subset:

- runtime lexer/parser for eval fragments without `<?php` tags;
- EvalIR/interpreter;
- basic expressions/statements;
- control flow;
- arrays and COW on supported paths;
- include/require;
- dynamic functions/classes and runtime metadata;
- interpreter-side builtin registry/dispatch;
- callable forms and `Closure::fromCallable`;
- classes, interfaces, traits, enums, static members, and reflection in the
  covered subset;
- throw/fatal/status handling where supported.

### Magician Performance

Completed:

- `scripts/benchmark_magician.py` benchmark suite with fixtures under
  `benchmarks/magician/cases/`;
- parse cache and parse-error cache;
- include cache with metadata validation;
- lookup cache for eval/native symbols;
- direct builtin dispatch for hot paths;
- conservative callable resolution cache;
- fewer `RuntimeValueOps` calls for output/simple scalars;
- temporary int/bool evaluator for assignment/return/condition;
- optional linear EvalIR for straight-line fragments;
- narrow fast paths for indexed-array writes.

### Literal Eval AOT

Completed:

- `EvalLiteralCall` preserves the literal payload in EIR;
- `src/eval_aot.rs` classifies eligibility and fallback reasons;
- `src/codegen/lower_inst/builtins/eval.rs` tries AOT before the bridge;
- support for scalars, arithmetic, concat/output, print, return, stores,
  read/write scope, and boxed Mixed scope paths;
- support for internal locals, assignments/compound assignments,
  while/if/break/continue, modulo, comparisons, and truthiness sufficient for
  the prime benchmark;
- support for common static builtins;
- support for known static functions;
- support for typed public static methods;
- support for static callbacks in `call_user_func()` and
  `call_user_func_array()`, including string, array, `Class::class`, and
  immediate first-class static forms;
- tests proving no `__elephc_eval_execute` call and no `elephc_magician` link
  for fully AOT fragments;
- prime-sum benchmark up to `100000` without the bridge, output `454396537`.

## Open Work

### 1. Converge AOT on Internal EIR Functions (completed)

Literal AOT fragments now lower exclusively as deterministic internal EIR
functions. The manual parser, local scalar IR, control-flow graph, and assembly
emitters were removed from `src/codegen/lower_inst/builtins/eval.rs`.

Implemented direction:

- represent each AOT fragment as an internal EIR function with a special ABI;
- declare fragment locals separately from caller locals;
- introduce EIR primitives or helper builtins for:
  - `eval_scope_get`;
  - `eval_scope_set`;
  - return/fallthrough `null`;
  - status/fatal propagation;
- send the AOT function through validation, optimization, register allocation,
  and the target-aware backend;
- keep magician fallback as the compatibility path.

Completed criteria:

- no manual mini-backend remains for new or existing literal AOT constructs;
- existing AOT tests continue to pass;
- the assembly marker remains explicit;
- no regression on macOS ARM64, Linux ARM64, or Linux x86_64.

### 2. Dynamic Names and Static Object/Member Specialization

The current AOT classifier deliberately treats object/member access and dynamic
names as fallback territory. That is correct until the compiler can prove the
same PHP-visible behavior as the magician fallback. The goal is not to make
dynamic PHP magically static everywhere; the goal is to recognize cases that are
only syntactically dynamic but semantically fixed at the call site or inside the
analyzed fragment.

Important example:

```php
class Car {
    private string $color;

    public function getProperty($property) {
        if (isset($this->$property)) {
            return $this->$property;
        }
        return null;
    }
}

$car = new Car();
echo $car->getProperty("color");
```

The optimization must not rewrite this as a source-level `echo $car->color` in
the caller context, because `color` is private. The safe rewrite is either an
inlined clone that preserves `Car` as the lexical access context, or a synthetic
specialized method/helper such as `Car::getProperty$specialized_color($this)`
whose property reads are still authorized by `Car`.

#### 2.1 Semantic Surfaces

Cover these PHP forms explicitly before allowing AOT:

- variable variables:
  - `$$name`;
  - `${$expr}`;
  - read, assignment, compound assignment, increment/decrement, by-reference
    binding, `isset`, `empty`, and `unset`;
  - local scope, function scope, method scope, and eval-created variables;
  - invalid or non-string-like names after PHP coercion.
- dynamic instance properties:
  - `$object->$name`;
  - `$object->{$expr}`;
  - `$object?->$name` and `$object?->{$expr}`;
  - read, assignment, compound assignment, array append/set, inc/dec,
    by-reference binding, `isset`, `empty`, and `unset`.
- dynamic static properties and members:
  - `$class::${$property}`;
  - `self::${$property}`, `static::${$property}`, and `parent::${$property}`;
  - static property read/write/isset/empty/unset where PHP permits it.
- member resolution rules:
  - declared public/protected/private properties;
  - private property slots attached to the declaring class;
  - inheritance and trait-origin metadata;
  - dynamic-property tails and `stdClass` properties;
  - typed properties, uninitialized typed properties, readonly properties, and
    nullable typed properties;
  - `__get`, `__set`, `__isset`, and `__unset`;
  - fatal/error behavior for inaccessible, undefined, or invalid accesses.

Anything outside the modeled surface stays in magician fallback with an explicit
fallback reason.

#### 2.2 Runtime Fallback Completion

The fallback remains the semantic oracle. Before AOT is extended, audit and fill
gaps in:

- `crates/elephc-magician/src/parser/` for all dynamic-name syntax accepted by
  the main parser;
- `crates/elephc-magician/src/eval_ir.rs` for distinct read/write/isset/unset
  operations instead of ad hoc expression handling;
- `crates/elephc-magician/src/interpreter/` for scope lookup, variable creation,
  object property lookup, static property lookup, magic methods, typed-property
  state, readonly checks, and by-reference aliases;
- `crates/elephc-magician/src/context.rs` for metadata needed to reflect AOT
  classes and native runtime classes accurately;
- native runtime hooks under `src/codegen_support/runtime/` when magician must
  call back into generated/AOT object layouts.

Fallback done criteria:

- every supported dynamic-name form has direct interpreter tests;
- PHP-equivalence tests cover visible output, return value, warnings/fatals, and
  mutation side effects where practical;
- unsupported forms fail or fall back deliberately rather than partially
  evaluating with the wrong scope or visibility.

#### 2.3 AOT Fact Model

Introduce a small, invalidation-aware fact model before changing the AOT
classifier. Required facts:

- known local string value: `$property === "color"`;
- known variable-variable target: `$$name` maps to `$color` only while `$name`
  and the target scope remain unchanged;
- exact object allocation: `$car` is exactly `new Car()` and cannot be an
  unknown subclass at that point;
- receiver method target: `$car->getProperty(...)` resolves to exactly
  `Car::getProperty`;
- constant argument facts at call sites;
- declared property identity: `Car::$color` as a property slot, not just the
  string `"color"`;
- lexical access context: the class scope that authorized the original
  property access;
- initialization/nullability facts only when they are already proven by
  existing type/flow analysis.

Facts must be invalidated by:

- assignments to any variable participating in the fact;
- variable variables that may alias the variable;
- by-reference calls, references, `global`, `static`, or unknown mutating calls;
- dynamic `eval`, include/require, or unknown callbacks;
- writes to object properties that may affect the resolved slot;
- any path where control-flow merge loses precision.

The first implementation can be local and conservative. It does not need a full
whole-program optimizer, but it must refuse ambiguous cases.

#### 2.4 Static Dynamic-Name AOT

After fallback semantics and facts exist, allow AOT only for narrow cases:

- `$$name` when `name` is a compile-time-known string and the target variable
  is a normal local/scope variable with no active reference ambiguity;
- `${$expr}` when `$expr` folds to the same safe known string;
- `$this->$property` inside a known class method when `$property` is a known
  string and resolves to a declared property visible from that method's lexical
  class;
- `$object->{$property}` when the object has an exact class fact, the property
  string is known, and the access is public or has an explicitly preserved
  lexical access context;
- `isset($object->$property)` / `empty($object->$property)` when the lowering can
  use a non-reading property probe that preserves PHP behavior for uninitialized
  typed properties and magic `__isset`;
- nullsafe dynamic property reads only when the null branch and non-null branch
  match PHP evaluation order and side effects.

Keep fallback for:

- unknown property names;
- unknown receiver classes;
- possible subclass overrides without an exact receiver or final method/class;
- inaccessible properties without a preserved lexical access context;
- magic methods unless the AOT path explicitly models their dispatch;
- references/by-ref paths until the ref-cell model is identical to fallback;
- dynamic static properties involving late static binding unless `self`/`static`
  context is precise.

#### 2.5 Access-Context-Aware Lowering

Private and protected properties require a lowering model that distinguishes
source context from call-site context.

Do not lower a specialized private read to a normal caller-side property access.
Instead, choose one of these representations:

- an internal EIR property operation carrying:
  - object value;
  - resolved property identity/slot;
  - lexical access context;
  - operation kind (`read`, `isset`, `write`, `unset`, etc.);
- or a synthetic specialized method/helper compiled with the original class as
  its access context.

Required invariants:

- property visibility is checked as if the original method body performed the
  access;
- private properties resolve to the declaring class slot, not to a public or
  child-class property with the same string name;
- protected properties preserve inheritance visibility rules;
- `isset` and `empty` do not accidentally read uninitialized typed properties;
- readonly and asymmetric property rules remain enforced on writes;
- magic methods are called only when PHP would call them;
- the generated path remains target-aware across macOS ARM64, Linux ARM64, and
  Linux x86_64.

#### 2.6 Call-Site Method Specialization

Specialization should be a separate phase from basic dynamic-name support.

Candidate requirements:

- receiver exactness:
  - exact allocation such as `$car = new Car()` in the same analyzable flow; or
  - final class/final method evidence strong enough to avoid virtual dispatch
    changes;
- method body available in the current compilation unit after includes are
  resolved;
- call arguments have stable constant facts or simple value facts;
- the method body does not contain unsupported constructs for the chosen
  specialization path;
- no by-reference parameters, references, dynamic `eval`, include/require,
  unknown callbacks, or global/static interactions unless explicitly modeled;
- parameter defaults, named arguments, variadics, and spread arguments have been
  normalized through the shared call-argument planner.

Implementation options:

1. Inline a cloned method body at the call site, preserving lexical class scope.
2. Generate a synthetic internal function/method keyed by receiver class,
   method, and constant-argument shape.

The first version should prefer synthetic helpers if that keeps access context,
cleanup, and debug markers easier to reason about.

Specialization must preserve:

- source evaluation order for receiver and arguments;
- method return semantics;
- `$this`, `self`, `static`, and `parent` resolution;
- visibility and property initialization checks;
- side effects before any early return or fatal;
- fallback behavior when any guard/fact is not available at compile time.

#### 2.7 Tests and Benchmarks

Add focused coverage in layers:

- parser tests for `$$name`, `${$expr}`, `$obj->$name`, `$obj->{$expr}`,
  nullsafe dynamic properties, and dynamic static properties;
- magician tests for dynamic variables and dynamic properties in normal reads,
  writes, `isset`, `empty`, `unset`, typed properties, private/protected/public
  properties, magic methods, and dynamic-property tails;
- codegen tests proving unsupported dynamic paths use magician fallback with a
  readable marker;
- AOT tests proving statically known dynamic names do not call
  `__elephc_eval_execute`;
- specialization tests for the `Car::getProperty("color")` shape, including:
  - private property access remains legal through preserved `Car` context;
  - uninitialized typed property returns the `isset`/`null` behavior;
  - initialized property returns the expected value;
  - subclass/override ambiguity falls back;
  - public dynamic property access still works without private-context rules;
- negative/error tests for inaccessible properties, readonly writes, invalid
  dynamic names, and magic-method edge cases;
- PHP cross-checks with `ELEPHC_PHP_CHECK=1` where behavior is subtle.

Benchmark only after correctness is in place. A useful manual benchmark would
compare:

- plain static property access;
- dynamic property access through magician;
- static dynamic-name AOT;
- call-site specialized getter.

#### 2.8 Done Criteria

This work is done only when:

1. fallback semantics cover the declared dynamic-name subset;
2. unsupported dynamic-name cases produce explicit fallback reasons;
3. AOT accepts only statically resolved dynamic names with precise facts;
4. private/protected property specialization preserves lexical access context;
5. call-site specialization never changes virtual dispatch, visibility,
   evaluation order, or error behavior;
6. tests cover runtime fallback, AOT acceptance, AOT rejection, and PHP-equivalent
   edge cases;
7. focused checks pass on all supported targets for any codegen/runtime changes.

### 3. General AOT Expansion Beyond Dynamic Names

Every new construct must be introduced only with a semantic model and tests.
Reasonable priority after the dynamic-name work:

1. arrays/iterables in AOT once COW and ownership are clear;
2. references/by-ref only if the ref-cell model is identical to runtime;
3. `global` and `static`;
4. `try`/`throw`;
5. include/require;
6. declarations inside eval.

Everything not modeled stays fallback.

### 4. Compiler/Eval Builtin Parity

`tests/builtin_parity_tests.rs` distinguishes:

- shared compiler/eval builtins;
- documented eval-only builtins;
- static-only builtins registered in the compiler but not yet exposed by
  magician.

When a static-only builtin is implemented in magician:

- remove it from the static-only allowlist;
- add eval signature metadata;
- add interpreter dispatch;
- add named/positional tests when relevant;
- update benchmarks only if the builtin enters an eval hot path.

### 5. Benchmarks and Measurement

The benchmark suite exists. Remaining work:

- decide which AOT benchmarks should become permanent;
- always exclude compile/assemble/link time from runtime numbers;
- keep at least one prime-loop case and one algebra-heavy case as a manual
  regression or CI artifact;
- preserve output correctness against PHP where practical.

### 6. Documentation

Update docs when the subset changes:

- eval enables an optional dynamic runtime;
- literal eval AOT does not embed the parser/compiler in the binary;
- magician fallback remains compatibility semantics;
- fully AOT programs do not link `elephc_magician`;
- constructs that still fall back should be documented when user-visible;
- allowed dual-frontend grammar divergences stay documented when user-visible.

### 7. Parser dual-stack governance

#### Current state

Eval has two intentional frontends:

1. **Compiler frontend** (`src/lexer/`, `src/parser/`) for literal AOT analysis
   and all ordinary compilation.
2. **Magician frontend** (`crates/elephc-magician/src/lexer/`,
   `crates/elephc-magician/src/parser/`) for runtime fragments, producing
   EvalIR for the interpreter.

Magician does not depend on the `elephc` crate. That keeps the optional
staticlib free of the full compiler pipeline and avoids shipping compile-time
machinery into user binaries.

#### Near-term work (required)

1. **Shared pure-PHP fragment grammar corpus**
   - one source list of fragments and negative cases;
   - must parse (or fail) consistently on main and magician for pure PHP;
   - where a fragment is a compile-time literal candidate, AOT acceptance must
     not disagree with magician on pure parse validity without an explicit
     fallback reason that is about semantics/AOT coverage, not parse success.
2. **Documented allowed divergences**
   - eval fragments reject opening `<?` / `<?php` tags;
   - magician rejects elephc-only extensions that the compiler accepts;
   - error and recovery models may differ (`CompileError` multi-error recovery
     vs magician `EvalParseError`);
   - any pure-PHP disagreement outside that list is a bug.
3. **Fix policy for PHP-visible syntax**
   - a main-parser fix that changes pure PHP acceptance for forms usable in
     eval must either update magician in the same change or add an explicit
     divergence note plus a corpus case;
   - prefer keeping pure PHP aligned over growing the divergence list.

#### Near-term done criteria

- the grammar corpus is checked in CI or an equivalent focused test gate;
- the allowed-divergence list lives in docs or this plan and is short;
- silent pure-PHP parse disagreements between main and magician are treated as
  regressions.

#### Non-goals

- do not extract or merge the full compiler parser/AST pipeline into magician
  before stable fallback lands on main;
- do not make magician depend on the main `elephc` crate;
- do not unify compile AST with EvalIR as a single IR;
- do not require a shared syntax crate as a merge prerequisite for eval.

#### Longer-term option (opportunistic)

If dual-parser maintenance cost stays high after fallback is on main, consider a
thin pure-PHP crate such as `elephc-php-syntax`:

- lexer + parser for pure PHP only (no `ptr` / `extern` / `packed` / `ifdef`);
- modes for full file vs eval fragment;
- no type checker, optimizer, codegen, or name-resolution pipeline;
- both elephc and magician depend on it;
- magician still lowers pure-PHP AST into EvalIR for interpretation.

This remains optional. Prefer corpus + policy first.

## Tests and Checks

For narrow AOT planner/lowering changes:

```bash
cargo check
cargo test --test codegen_tests literal_eval_static
cargo test --test codegen_tests test_literal_eval_prime_loop_uses_aot_without_execute_bridge
git diff --check
```

For runtime bridge or interpreter changes:

```bash
cargo check
cargo test -p elephc-magician <filter>
cargo test --test codegen_tests eval_<filter>
git diff --check
```

For dynamic-name or object/member specialization changes:

```bash
cargo check
cargo test -p elephc-magician dynamic_property
cargo test -p elephc-magician variable_variable
cargo test --test codegen_tests eval_dynamic
cargo test --test codegen_tests literal_eval_static
git diff --check
```

For ABI/codegen/runtime ownership changes:

```bash
cargo check
cargo test --test codegen_tests <focused_eval_filter>
./scripts/test-linux-x86_64.sh <focused_eval_filter>
./scripts/test-linux-arm64.sh <focused_eval_filter>
git diff --check
```

For grammar dual-frontend parity changes (once the corpus exists):

```bash
cargo check
cargo test <grammar_parity_filter>
cargo test -p elephc-magician <grammar_parity_filter>
git diff --check
```

For manual benchmarks:

```bash
python3 scripts/benchmark_magician.py --case algebra_heavy --iterations 5 --warmup 1
python3 scripts/benchmark_magician.py --case literal_scalar_aot --iterations 5 --warmup 1
```

## Risks

- Incomplete scope sync can create stale variables or miss creations/unsets.
- Duplicating manual AOT codegen creates a second backend that is hard to
  maintain.
- Treating eval as ordinary static code can break PHP eval semantics.
- References, COW, arrays, and object properties can introduce double-free,
  leaks, or missed mutations if they bypass runtime helpers.
- `eval('$x + 1;')` returns `null`, not the last expression.
- Over-aggressive fallback selection can miscompile dynamic code.
- Variable variables can invalidate otherwise local-looking facts.
- Dynamic property specialization can break private/protected visibility unless
  lexical access context is preserved explicitly.
- `isset`/`empty` over typed properties must not be lowered into reads that
  fatal on uninitialized state.
- Magic property methods can turn apparently local object access into arbitrary
  user code.
- Magician optimizations must not freeze context/scope/magic constants.
- Dual frontends can accept or reject different pure PHP fragments, so AOT
  (compiler parser) and runtime fallback (magician parser) can silently
  diverge without a grammar corpus and fix policy.
- Prematurely merging parsers or making magician depend on the full compiler
  can bloat user binaries and create circular crate dependencies.
- Every new path must stay target-aware on macOS ARM64, Linux ARM64, and Linux
  x86_64.

## Final Completion Criteria

The eval/magician work can be considered closed when:

1. magician fallback covers the declared PHP subset with tests;
2. every supported literal eval uses AOT or an explicit fallback reason;
3. the AOT subset does not depend on an unmaintainable manual mini-backend;
4. fully AOT programs do not link `elephc_magician`;
5. static/eval builtin parity has no stale allowlist entries;
6. dynamic-name and object/member specializations preserve lexical access
   context, visibility, and PHP fallback behavior;
7. prime-loop and algebra-heavy benchmarks remain correct and measurable;
8. all three supported targets have focused coverage for every ABI/codegen
   change;
9. docs and tests exactly reflect the supported subset and fallbacks;
10. pure-PHP fragment grammar is either shared by a thin syntax crate or kept
    aligned by a green dual-frontend corpus with a short, documented
    divergence list (full parser unification is not required).
