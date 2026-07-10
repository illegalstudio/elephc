# Memory-model-aware propagation and targeted invalidation

> **ROADMAP item (v0.26.x):** Memory-model-aware propagation for heap-backed
> locals and targeted runtime invalidations beyond `unset($var)` and the
> currently modeled local writes.

## Context

AST constant propagation (`src/optimize/propagate/`) tracks a
`ConstantEnv = HashMap<String, ScalarValue>` per program point. Two structural
limits remain after "Alias-aware constant propagation" and "Constant
propagation v2" (both shipped):

1. **Scalar-only facts.** Heap-backed locals (arrays) never carry facts, so
   `$a = [1, 2, 3]; echo $a[1];` does not fold, even though the in-place
   literal fold (`try_fold_array_access`, shipped in v2) folds
   `[1, 2, 3][1]` today.
2. **Blanket invalidation.** The only *targeted* invalidations are
   `unset($var)` on plain variables and the modeled local writes
   (`expr_local_writes` / `stmt_local_writes`). Everything else — any call,
   any side-effecting expression, `unset($a[0])`, an unfolded array read
   (whose `Effect` carries `may_warn`) — clears the entire environment.
   `propagate_constants()` also runs without the callable-effects maps
   installed, so even a call to a pure user function wipes all facts.

Both limits are memory-model questions: *who can actually write this local?*
In PHP's memory model a callee cannot write a caller's plain local except
through an exposed reference (by-ref parameter, by-ref closure capture,
`$x = &…` alias, `global`/`static` binding, superglobal) — and arrays have
value semantics (assignment snapshots via COW), so a name-keyed array fact
stays valid across writes to *other* names.

## Goal

Extend `src/optimize/propagate/` so that:

- locals holding all-scalar array literals carry an array fact, and
  `$a[<const>]` folds through the existing `try_fold_array_access`;
- side-effecting statements/expressions invalidate **only the names they can
  write**, falling back to full clearing only for genuinely unknowable writes;
- every reference-exposure point marks names volatile so they never carry
  facts (the memory-model justification for the targeted rules above).

## Non-goals

- EIR-level heap forwarding (peephole `load_store.rs` keeps its `NonHeap` +
  full-barrier model; lifting it interacts with refcount balance, COW
  ensure-unique, and backend call-clobber assumptions — separate work).
- Loop fixed-point propagation, object/property facts, nested-array facts,
  `count()`/`isset()` folding over facts, and `$GLOBALS` (unsupported).
- Changing `Effect`-based DCE/pruning observability decisions. `Effect` keeps
  answering "is this observable?"; the new analysis answers "which locals can
  this write?". The two are deliberately separate.

## Alternatives considered

- **EIR peephole extension** (forward heap values, use `Effects` bits as
  targeted barriers): precise IR effects exist, but heap forwarding must
  reason about acquire/release balance, COW ensure-unique reallocation behind
  pointers, and extending live ranges across calls (explicitly warned against
  in `load_store.rs`). High miscompile risk, and the ROADMAP item's siblings
  (alias-aware propagation, propagation v2, purity v2) are all AST-track.
  Rejected.
- **Hybrid** (AST work + EIR barrier refinement): the EIR half alone requires
  a backend call-clobber audit; deferred as follow-up work.
- **AST-level extension** (chosen): builds on shipped, tested infrastructure —
  `ConstantEnv`, `expr_local_writes`, `REFERENCE_VOLATILE`, the callable-effects
  fixed point, and `try_fold_array_access`.

## Design

### 1. `Invalidation` analysis (`propagate/writes.rs`)

```rust
enum Invalidation {
    Names(HashSet<String>),  // exactly these locals may be written
    All,                     // unknowable — clear the environment
}
```

New `expr_invalidation(expr)` / `stmt_invalidation(stmt)` mirror the existing
write collectors but replace most `None` (= All) cases with precise rules:

- **Known local writes** stay as today (assignments, increments, foreach
  vars, …): `Names(writes)`.
- **`unset` beyond plain variables:** `unset($a[0])`, `unset($a[$i][$j])` →
  the root variable (`$a`); `unset($obj->prop)`, `unset(Class::$p)` → no
  local written. Argument sub-expressions still contribute their own
  invalidation.
- **Array reads** (`$a[$i]`) write no locals → contribute nothing (their
  `may_warn` observability is `Effect`'s concern, not the env's).
- **Calls** resolve the callee and take the union of:
  - *by-ref argument roots*: plain-variable (or array-access-root) arguments
    sitting at by-ref parameter positions. Positions come from a program
    signature pre-scan for user functions/methods and from
    `BuiltinDef.ref_params` for builtins (variadic by-ref covers the tail).
    Named arguments match by parameter name; a spread into a callee with any
    by-ref parameter → `All`.
  - *reference retention*: user-defined callees may stash the reference, so
    by-ref argument roots are also marked volatile. Builtins do not retain —
    they invalidate at the call only.
  - *unknown callees* (closure vars, expr calls, pipes, dynamic new): every
    plain-variable argument is invalidated and marked volatile; `All` is not
    needed because a callee still cannot reach non-exposed locals.
  - *top-level scope*: top-level locals are globals, so a callee whose
    effect has `writes_globals` (or an unknown callee) → `All`. Inside
    function bodies this is unnecessary: `global`-bound names are volatile.
  - *method calls*: by-ref positions are the union across all same-named
    methods in the program (dynamic dispatch safe). `new` uses the union of
    `__construct` signatures; if any class declares a by-ref property or
    by-ref ctor param, `new` invalidates and volatilizes all plain-variable
    arguments.
- **`include` / `yield` / `yield from`** stay `All` (include splices code into
  the current scope; generator resumption is unknowable).

Statement sites that currently clear the env on `has_side_effects`
(`env_after_expr_side_effects`, `ExprStmt`, `Echo`, `NestedArrayAssign`,
assignment RHS handling in `env_after_scalar_assign`, `propagate_expr`'s
`must_clear`) switch to subtracting `Invalidation::Names` and clearing only on
`All`.

### 2. Volatility = the memory-model ledger (`propagate/stmt.rs`)

`REFERENCE_VOLATILE` (names that must never carry facts) grows new marking
sites, each a reference-exposure point:

| Site | Marks volatile |
|---|---|
| `$t = &$s` (`RefAssign`) | `$t`, `$s`/root of `$s` lvalue (today: only plain-variable `$s`) |
| `foreach ($a as &$v)` | `$a` (array root: writes through `$v` mutate it invisibly, and `$v` keeps aliasing the last element after the loop; `$v`'s *own* value facts stay sound and need no mark) |
| closure `use (&$x)` | `$x` at closure-creation |
| `global $x;` | `$x` (aliases global storage; callees can write it) |
| `static $x;` | `$x` (recursive calls write the shared cell) |
| by-ref arg to user-defined callee | argument root (callee may retain the reference) |
| `ptr($x)` (pointer extension) | `$x` (address taken: `ptr_set` through any alias rewrites it outside the PHP reference model) |
| request superglobals (`_GET`, …) | always (writable from any scope under `--web`) |

Volatile names never enter the env, so calls need no extra invalidation for
them — this is what makes the targeted call rule sound.

### 3. `Effect.writes_globals` + installing effects for propagation

- `Effect` gains a third bit, `writes_globals`, set when a body contains a
  `global` declaration (conservative: declaring ≈ writing) and propagated
  transitively by the existing `compute_program_callable_effects` fixed point.
  Conservative fallbacks (`unknown callee`) include it; builtins never set it
  (no builtin writes PHP globals — enforced by a parity test).
- `pipeline.rs` wraps `propagate_constants` in `with_callable_effects(...)`
  like the other optimizer passes, so pure user functions stop clearing the
  env and `writes_globals` is queryable at top level.
- **By-ref substitution hazard:** with effect maps installed, a pure callee
  with a by-ref parameter (`function f(&$x) { return $x + 1; }`) would let
  `propagate_args` substitute a literal into a by-ref position — invalid
  (by-ref needs an lvalue). `propagate_args` must mask by-ref positions using
  the same signature pre-scan/registry data. A parity test asserts no builtin
  with `ref_params` is classified pure-non-throwing.
- Scope tracking: a small helper (`with_function_scope`, thread-local like the
  existing maps) distinguishes top-level from function/method/closure bodies.

### 4. Array facts for heap-backed locals (`ConstantEnv` value type)

```rust
enum PropagatedValue {
    Scalar(ScalarValue),
    ArrayLit(Expr),  // ArrayLiteral / ArrayLiteralAssoc, all keys+values scalar literals, ≤ 64 elements
}
```

- **Creation:** `Assign`/`TypedAssign` whose folded RHS is a qualifying array
  literal records `ArrayLit` (name not volatile). `$b = $a` where `$a` holds a
  fact copies it — COW value semantics make the snapshot sound. By-value
  closure captures snapshot the same way (`captured_constant_env`).
- **Consumption:** in `propagate_expr`'s `ArrayAccess` arm, when the array is
  a variable with an `ArrayLit` fact, delegate to `try_fold_array_access`
  (same helper the in-place literal fold uses) and commit only its scalar
  result. Variables holding `ArrayLit` are **never** substituted at plain
  `Variable` sites (materializing a literal would break by-ref argument
  passing and duplicate allocations).
- **Invalidation:** the same machinery as scalars — element writes
  (`ArrayAssign`, `ArrayPush`, `NestedArrayAssign` root), `unset`, by-ref
  exposure, calls with the variable at a by-ref position (e.g. `sort($a)`)
  all remove the name. `list()` unpack values keep working (elements are
  scalars). Env merges compare `PropagatedValue` by structural equality
  (`merge_constant_env_paths` and friends are type-generic already).
- The ≤ 64-element cap bounds env copying across merges; larger literals
  simply carry no fact.

## Correctness argument (memory model)

A caller-visible local can change only through: (1) a direct local write —
modeled by `expr_local_writes`; (2) a write through an alias — every alias
creation point marks the name volatile, so it never carries a fact; (3) a
callee writing through a by-ref parameter — invalidated (and volatilized for
user callees) at the call site; (4) global-storage writes — volatile inside
functions (`global`, superglobals), `writes_globals`-guarded at top level;
(5) engine-level mutation of array contents behind a fact — impossible for a
fact-carrying name: array facts are created from literals and killed at every
write/exposure point, and PHP array assignment has value semantics (COW), so
no other name aliases the fact's storage.

Marking is flow-ordered within the linear propagation walk (an alias created
*after* a use cannot invalidate the earlier use), and `REFERENCE_VOLATILE`
stays name-keyed and program-wide — cross-scope collisions only lose
precision, matching the existing conservative contract.

## Testing

Unit (`src/optimize/tests/propagate.rs`):
- array fact folds `$a[1]`/`$a['k']`; no fold for unknown index, oversized or
  non-scalar literals; nested access does not materialize literals;
- COW: `$b = $a; $b[0] = 9;` keeps `$a`'s fact; `$a[0] = 9` kills only `$a`;
- targeted calls: pure user fn keeps env; output-only builtin keeps env;
  `sort($a)` kills `$a` but keeps `$x`; by-ref user fn volatilizes; unknown
  callee (`$f()`) kills only variable args; top-level call to `global`-writing
  fn clears; in-function `global $x` volatilizes `$x` only;
- `unset($a[0])` kills `$a` only; `unset($o->p)` keeps env;
- foreach-by-ref volatilizes value var and array; closure `use (&$x)`
  volatilizes; `static $x` volatilizes;
- by-ref substitution mask: pure `f(&$x)` call keeps `$x` as a variable arg.

E2E (`tests/codegen/optimizer/`): compile-and-run programs exercising each
hazard (aliasing, sort-by-ref, COW copies, global-writing callee, foreach
by-ref, closure by-ref capture) asserting PHP-checked stdout, so any unsound
fold or missed invalidation shows up as a behavior diff. Builtin parity test
for ref-params ∉ pure list (`src/builtins/parity_tests.rs` or
`src/optimize/tests/`).

## Docs and bookkeeping

- `docs/internals/the-optimizer.md`: describe the propagation env (facts +
  invalidation + volatility), remove the "memory-model-aware propagation"
  bullet from "What the optimizer does not do yet".
- `ROADMAP.md`: tick the item.
- `CHANGELOG.md` is release-managed; no entry now.

---

# Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the design above: targeted local invalidation + array facts for heap-backed locals in `src/optimize/propagate/`.

**Architecture:** Bottom-up, every commit green. Volatility marks land first (pure precision-loss, protected today by blanket clears), then the `writes_globals` effect bit, the by-ref signature pre-scan, the `propagate_args` mask (hazard gate), installing effect maps for propagation, the `Invalidation` analysis, the wiring switch, the `PropagatedValue` refactor, array facts, e2e hazard tests, docs.

**Tech stack:** Rust; no new dependencies. Unit tests via AST builders (`Stmt::assign`, `Expr::int_lit`, …) in `src/optimize/tests/propagate/`; e2e via `compile_and_run` in `tests/codegen/optimizer/`.

## Global constraints (from AGENTS.md)

- Zero compiler warnings (`cargo build` clean); never run `cargo fmt`.
- Every new `.rs` file needs the `//!` module preamble; every function a `///` docblock.
- Focused tests only (`cargo test --test <binary> <filter>` / `cargo test <filter>`); full suite is CI's job.
- Commit prefixes `feat:`/`fix:`/`test:`/`refactor:`/`docs:`; concise messages; no Co-Authored-By.
- PHP-visible behavior must be identical before/after each task (propagation is semantics-preserving); cross-check foldings with `php -r` when in doubt.

### Task 1: Volatility ledger extensions

**Files:**
- Modify: `src/optimize/propagate/stmt.rs` (RefAssign, Global, StaticVar arms; reset helper)
- Modify: `src/optimize/propagate/stmt/control.rs` (`propagate_foreach_stmt`)
- Modify: `src/optimize/propagate/expr.rs` (Closure arm)
- Modify: `src/optimize/propagate/writes.rs` (new `lvalue_root` helper)
- Test: `src/optimize/tests/propagate/straight_line.rs` (or a new `volatility.rs` submodule)

**Interfaces:**
- Produces: `pub(crate) fn lvalue_root(expr: &Expr) -> Option<&str>` in `writes.rs` (Variable → name; NamedArg/ArrayAccess → recurse into value/array; everything else → None). `mark_reference_volatile` becomes `pub(super)` so `control.rs`/`expr.rs` can call it.
- Marks: `global $x` → `$x`; `static $x` → `$x`; `foreach ($a as &$v)` → root of `$a` only; closure `use (&$x)` → `$x`; `$t = &<lvalue>` → `$t` + `lvalue_root(source)`; the seven `crate::superglobals::SUPERGLOBALS` names at the start of `propagate_constants` (after `reset_reference_volatile()`).

- [ ] Write failing tests (representative; each is a full `#[test]` with AST fixtures in the existing style):

```rust
/// `static $x = 0; $x = 5; echo $x + 1;` must NOT fold the echo: a recursive
/// call can rewrite the shared static cell, so `x` never carries facts.
#[test]
fn test_static_var_blocks_propagation() {
    let program = vec![
        Stmt::new(StmtKind::StaticVar { name: "x".into(), init: Expr::int_lit(0) }, Span::dummy()),
        Stmt::assign("x", Expr::int_lit(5)),
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1))),
    ];
    let propagated = propagate_constants(program);
    assert_eq!(
        propagated[2],
        Stmt::echo(Expr::binop(Expr::var("x"), BinOp::Add, Expr::int_lit(1)))
    );
}
```

Same-shape tests: `global $x;` then assign+echo (no fold); `foreach ($a as &$v) {}` then `$a`-fact… (deferred to Task 9 — for now assert `$v = 9; echo $v + 1;` after the loop still folds to `10`, locking the "no `$v` mark" decision); closure `use (&$x)` then `$x = 5; echo $x + 1;` (no fold); `$t = &$a[0];` then `$a`-sensitive echo — for now assert `$t` stays unpropagated after `$t = 5`.
- [ ] Run: `cargo test --lib optimize::tests::propagate` → new tests FAIL (folds still happen).
- [ ] Implement the marks listed under Interfaces. `Global`/`StaticVar` arms add `mark_reference_volatile(...)` before their existing `env.remove(...)`; `propagate_foreach_stmt` marks the array root when `value_by_ref`; the `Closure` arm in `propagate_expr` marks each `capture_refs` entry before propagating the body; `RefAssign` extends its existing source handling with `lvalue_root`.
- [ ] Run: `cargo test --lib optimize::tests` → PASS (fix any existing expectation that relied on folding a `global`/`static` name — semantics allow only precision loss).
- [ ] `cargo build` warning-free; commit `feat(optimize): mark reference-exposed names volatile in propagation`.

### Task 2: `Effect.writes_globals` + builtin purity parity test

**Files:**
- Modify: `src/optimize.rs` (`Effect` struct, `PURE`, `combine`, new `with_writes_globals`)
- Modify: `src/optimize/effects.rs` (`StmtKind::Global` arm)
- Modify: `src/optimize/effects/calls.rs` (conservative fallbacks)
- Test: `src/optimize/tests/effects.rs`, `src/builtins/parity_tests.rs`

**Interfaces:**
- Produces: `Effect { has_side_effects, may_throw, writes_globals }`, `Effect::with_writes_globals(self) -> Self`; `combine` ORs the bit; `is_observable`/`is_pure` semantics unchanged (the bit does not affect DCE).
- Rules: `StmtKind::Global` → `Effect::PURE.with_writes_globals()` (declaring ≈ writing, conservative); every conservative unknown-callee fallback in `calls.rs` (`function_call_effect` non-builtin fallback, `callable_alias_effect`, `expr_call_effect` `_` arm, `static_method_call_effect` fallbacks, `private_instance_method_call_effect` fallbacks) adds `.with_writes_globals()`; known builtins never set it (registry lookup: if `crate::builtins::registry::lookup(name).is_some()`, the non-pure fallback stays `.with_side_effects().with_may_throw()` without the bit).

- [ ] Write failing tests: `compute_program_callable_effects` (exercise via `normalize_control_flow`-style setup or a direct unit test in `src/optimize/tests/effects.rs`) reports `writes_globals` for `function f() { global $g; $g = 1; }` and transitively for `function h() { f(); }`; reports `!writes_globals` for a pure function and for a body calling only `strlen`. Parity test in `src/builtins/parity_tests.rs`: for every registry builtin, `is_pure_non_throwing_builtin(name)` implies `def.ref_params.iter().all(|r| !r)` (expose the predicate or duplicate the name list check via a small `pub(crate)` accessor — prefer making `is_pure_non_throwing_builtin` `pub(crate)`).
- [ ] Run: `cargo test --lib optimize::tests::effects builtins::parity` → FAIL (field missing).
- [ ] Implement the struct/bit changes above.
- [ ] Run the same filters → PASS; `cargo build` clean; commit `feat(optimize): track writes_globals in callable effect summaries`.

### Task 3: By-ref signature pre-scan

**Files:**
- Create: `src/optimize/propagate/signatures.rs`
- Modify: `src/optimize/propagate/mod.rs` (`mod signatures; pub(crate) use signatures::*;`)
- Modify: `src/optimize.rs` (`propagate_constants` installs the scan)
- Test: `src/optimize/tests/propagate/signatures.rs` (new submodule)

**Interfaces (produces):**

```rust
/// Per-callee by-ref parameter summary: (param name, is_by_ref) in order.
pub(crate) struct ByRefSignatures {
    pub functions: HashMap<String, Vec<(String, bool)>>,          // user FunctionDecls
    pub methods_by_name: HashMap<String, Vec<(String, bool)>>,    // union across classes/traits/enums, incl. __construct
    pub any_ctor_by_ref: bool,                                    // any by-ref ctor param OR any by-ref property
}
pub(crate) fn collect_by_ref_signatures(program: &[Stmt]) -> ByRefSignatures;
pub(crate) fn with_by_ref_signatures<R>(sigs: ByRefSignatures, f: impl FnOnce() -> R) -> R; // thread-local install
pub(crate) fn function_by_ref_params(name: &str) -> Option<Vec<(String, bool)>>;  // user map, then registry (BuiltinDef.params × ref_params)
pub(crate) fn method_by_ref_params(name: &str) -> Option<Vec<(String, bool)>>;
pub(crate) fn any_ctor_by_ref() -> bool;
pub(crate) fn is_user_function(name: &str) -> bool;               // retention: user callees volatilize by-ref roots
```

Union rule for `methods_by_name`: element-wise OR of the by-ref flags; when lengths differ, extend with the longer signature's entries. Registry lookups return `Some` even when the thread-local is unset (builtins are static). `FunctionVariantGroup` variants contribute like plain `FunctionDecl`s.

- [ ] Write failing tests: a program with `function f(&$a, $b) {}`, a class + trait each declaring `m(&$x)` / `m($x, &$y)` (assert unioned `[(“x”, true), (“y”, true)]`), a by-ref promoted/declared property setting `any_ctor_by_ref`, and `function_by_ref_params("sort")` returning the registry's by-ref first param.
- [ ] Run: `cargo test --lib optimize::tests::propagate::signatures` → FAIL.
- [ ] Implement `signatures.rs` (walk `StmtKind::FunctionDecl`/`ClassDecl`/`TraitDecl`/`EnumDecl`/`InterfaceDecl`/`FunctionVariantGroup`/`NamespaceBlock`; params tuple is `(String, Option<TypeExpr>, Option<Expr>, bool)` — the bool is `is_ref`; properties expose `by_ref`). Install in `propagate_constants`:

```rust
pub fn propagate_constants(program: Program) -> Program {
    reset_reference_volatile();
    for name in crate::superglobals::SUPERGLOBALS { mark_reference_volatile(name); } // from Task 1
    let sigs = collect_by_ref_signatures(&program);
    with_by_ref_signatures(sigs, || propagate_block(program, HashMap::new()).0)
}
```

- [ ] Run tests → PASS; `cargo build` clean; commit `feat(optimize): collect by-ref parameter signatures for propagation`.

### Task 4: `propagate_args` by-ref mask

**Files:**
- Modify: `src/optimize/propagate/expr.rs` (`propagate_args` + call sites)
- Test: `src/optimize/tests/propagate/straight_line.rs`

**Interfaces:**
- `propagate_args(args, env, by_ref: Option<&[(String, bool)]>)` — third parameter; an argument sitting at a by-ref position (positional index, or `NamedArg` matched by param name; `Spread` never matches) is passed through **unchanged** (no substitution, no folding — it must stay an lvalue). Call sites: `FunctionCall` passes `function_by_ref_params(name)`; `MethodCall`/`NullsafeMethodCall`/`StaticMethodCall` pass `method_by_ref_params(method)`; `NewObject`-family passes `method_by_ref_params("__construct")`; `ClosureCall`/`ExprCall` pass `None` paired with their existing conservative `arg_env` (unknown callees still get no substitution because the alias-effect fallback keeps `has_side_effects`).

- [ ] Write failing test: `function f(&$x) { return $x + 1; }` is *pure* by effect analysis; with the Task 5 maps this would substitute — lock the invariant now with a builtin: `$n = 3; sort($n_arr_placeholder)` is unwieldy, so test directly that `propagate_args` leaves `Expr::var("x")` untouched at a by-ref position while substituting at a by-value one (unit-test the function through a `FunctionCall` fixture whose signature comes from a `with_by_ref_signatures` install).
- [ ] Run → FAIL; implement; run → PASS; `cargo build` clean; commit `feat(optimize): never substitute constants into by-ref argument positions`.

### Task 5: Install callable effects for propagation

**Files:**
- Modify: `src/pipeline.rs:243` and `src/optimize.rs` (`propagate_constants`)
- Test: `src/optimize/tests/propagate/straight_line.rs`

**Interfaces:**
- `propagate_constants` computes `compute_program_callable_effects(&program)` and wraps the walk in `with_callable_effects(...)` (inside the Task 3 signature install). `pipeline.rs` needs no change if the wrap lives in `propagate_constants` — keep it there so tests get it for free.

- [ ] Write failing test: `function pf($a) { return $a + 1; } $x = 5; pf(1); echo $x + 1;` — the echo must fold to `6` (pure user call no longer clears the env). Keep a companion test that an *impure* user call (body echoes) still clears.
- [ ] Run → FAIL; implement the wrap; run `cargo test --lib optimize::tests` → all PASS (Task 4's mask is what makes this safe); commit `feat(optimize): use callable effect summaries during constant propagation`.

### Task 6: `Invalidation` analysis module

**Files:**
- Create: `src/optimize/propagate/invalidation.rs`
- Modify: `src/optimize/propagate/mod.rs`, `src/optimize/propagate/stmt.rs` (scope flag helper)
- Test: `src/optimize/tests/propagate/invalidation.rs`

**Interfaces (produces):**

```rust
pub(crate) enum Invalidation { Names(HashSet<String>), All }
impl Invalidation {
    pub(crate) fn none() -> Self;
    pub(crate) fn union(self, other: Self) -> Self;               // All absorbs
    pub(crate) fn add(&mut self, name: &str);
    pub(crate) fn apply(&self, env: &mut ConstantEnv);            // Names → remove each; All → clear
}
pub(crate) fn expr_invalidation(expr: &Expr) -> Invalidation;
pub(crate) fn block_invalidation(body: &[Stmt]) -> Invalidation;  // for loop helpers
pub(crate) fn stmt_invalidation(stmt: &Stmt) -> Invalidation;
// scope flag (in stmt.rs, thread-local Cell<bool> like REFERENCE_VOLATILE):
pub(super) fn with_function_scope<R>(f: impl FnOnce() -> R) -> R;
pub(super) fn in_function_scope() -> bool;
```

`expr_invalidation` fast path: `expr_local_writes(expr)` returning `Some(w)` ⇒ `Names(w)` (a `Some` proves no calls/`unset`-complex/pipe/yield anywhere inside). Otherwise a full recursive match with these interesting arms (everything else unions its children):

- `FunctionCall("unset", args)`: per arg — `Variable` → name; `ArrayAccess` chain → `lvalue_root` + `expr_invalidation` of indices; `PropertyAccess`/`StaticPropertyAccess`/dynamic-property → children only (no local written); any other shape → `All`.
- `FunctionCall(name, args)`: args' own invalidations ∪ `call_args_invalidation(function_by_ref_params(name), args, retain: is_user_function(name))` ∪ top-level guard: `if !in_function_scope() && function_call_effect(name).writes_globals { All }`.
- `MethodCall`/`NullsafeMethodCall`/`StaticMethodCall`: same with `method_by_ref_params(method)`; top-level guard uses the corresponding `*_method_call_effect` lookups; `retain: true`.
- `NewObject`-family: children ∪ (if `any_ctor_by_ref()` → invalidate+volatilize every plain-variable arg via `method_by_ref_params("__construct")` when available, else all variable args); top-level guard: conservative `All` at top level (ctor effects are not per-class yet — matches today's blanket clear there).
- `ClosureCall`/`ExprCall`/`Pipe`: unknown callee — invalidate + volatilize every `lvalue_root`-bearing argument; at top level → `All`. Exception: `ExprCall` on an inline `Closure` uses `closure` knowledge only for the top-level guard via `expr_call_effect`; args still treated unknown-callee (closure params may be by-ref — readable from the `Closure` node's params as a precision bonus, take it if cheap).
- `Assignment { target, value, prelude }`: `collect_assignment_target_writes` names ∪ children.
- `Yield`/`YieldFrom` → `All`.

`call_args_invalidation(sig, args, retain)`: positional cursor skips `NamedArg` (matched by name) and treats `Spread` as `All` **iff** the sig has any by-ref entry, else it contributes nothing; a by-ref position's `lvalue_root` gets invalidated and, when `retain`, `mark_reference_volatile`d; args beyond the signature are by-value. `sig == None` (unknown user symbol): invalidate + volatilize every `lvalue_root`-bearing arg.

`stmt_invalidation`: known statements delegate to component `expr_invalidation`s plus their own written names (mirror `stmt_local_writes` structure); `Include`/`FunctionVariantMark`/`IncludeOnceMark` → `All`.

- [ ] Write failing unit tests calling `expr_invalidation` directly under `with_by_ref_signatures`/`with_callable_effects` installs: `unset($a[0])` → `Names({a})`; `unset($o->p)` → `Names({})`; `sort($a)` → `Names({a})` without volatilizing; user `f($x)` with `f(&$p)` → `Names({x})` **and** `is_reference_volatile("x")`; `strlen($s)` → `Names({})`; `$f($x)` → `Names({x})` + volatile; top-level `g()` where `g` declares `global` → `All`; the same inside `with_function_scope` → `Names({})`.
- [ ] Run: `cargo test --lib optimize::tests::propagate::invalidation` → FAIL; implement; PASS; `cargo build` clean; commit `feat(optimize): add targeted local-write invalidation analysis`.

### Task 7: Wire invalidation into every clear site

**Files:**
- Modify: `src/optimize/propagate/stmt.rs` (`env_after_expr_side_effects` → `env_after_invalidation`; `ExprStmt`, `Echo`, `NestedArrayAssign`, `Assign`-family arms; wrap function/method/closure bodies in `with_function_scope` — `FunctionDecl` arm, `declarations.rs` `propagate_method`, `expr.rs` `Closure` arm)
- Modify: `src/optimize/propagate/stmt/env.rs` (`env_after_scalar_assign`, `env_after_list_unpack`)
- Modify: `src/optimize/propagate/expr.rs` (`must_clear` → subtract `expr_invalidation`)
- Modify: `src/optimize/propagate/stmt/control.rs:184,275,286`, `src/optimize/propagate/simulate.rs:196,231` (condition guards)
- Modify: `src/optimize/propagate/writes.rs` (`safe_loop_env`/`safe_foreach_env` fall back to `block_invalidation` instead of bailing to empty)
- Test: `src/optimize/tests/propagate/` (new `targeted_invalidation.rs`)

**Interfaces:**
- Replaces `env_after_expr_side_effects(env, &[&e])` with `env_after_invalidation(env, &[&e])` = union of `expr_invalidation` applied via `Invalidation::apply`. `propagate_expr`'s `must_clear` becomes: compute `expr_invalidation(&expr)`; `All` → empty env for substitution; `Names(w)` → substitute against `env` minus `w` (within-expression write ordering stays conservative). `Throw`'s empty-env return and try/catch env resets are control-flow conservatism — **keep them**.

- [ ] Write failing tests (each a full fixture): `$x = 5; echo f($y); echo $x + 1;` folds the last echo when `f` is a user function without by-ref params but with output (today: cleared); `$x = 5; sort($a); echo $x + 1;` folds while `$a`-dependent reads don't; `$x = 5; $y = $a[0]; echo $x + 1;` folds (array read no longer clears); loop bodies containing a no-by-ref call keep pre-loop facts for unwritten vars (`safe_loop_env` upgrade); `unset($a[0]); echo $x + 1;` folds.
- [ ] Run → FAIL; implement site by site, keeping `cargo test --lib optimize::tests` green at each sub-step (the old blanket behavior is the `All` degenerate case, so sites can switch incrementally).
- [ ] Full lib-test filter PASS; `cargo build` clean; commit `feat(optimize): targeted invalidation replaces blanket env clearing in propagation`.

### Task 8: `PropagatedValue` refactor (mechanical)

**Files:**
- Modify: `src/optimize.rs` (`type ConstantEnv = HashMap<String, PropagatedValue>;` + enum next to it or in `fold/scalar.rs`)
- Modify (compiler-driven): `src/optimize/propagate/{expr.rs, stmt/env.rs, simulate.rs, stmt/control.rs}` — every `ScalarValue` env read/write wraps/matches `PropagatedValue::Scalar`
- Test: existing suite only

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq)]
pub(in crate::optimize) enum PropagatedValue {
    Scalar(ScalarValue),
    ArrayLit(Expr),   // guaranteed qualifying array literal (Task 9 creates them)
}
```

Variable substitution site matches `Scalar(v) → v.clone().into_expr_kind()`, `ArrayLit(_) → keep the variable`. No `ArrayLit` is constructed yet — behavior identical.

- [ ] `cargo build` — follow the compiler errors through every site; `cargo test --lib optimize::tests` → PASS unchanged; commit `refactor(optimize): generalize ConstantEnv values to PropagatedValue`.

### Task 9: Array facts

**Files:**
- Modify: `src/optimize/propagate/stmt/env.rs` (`env_after_scalar_assign` creation + copy), `src/optimize/propagate/expr.rs` (`ArrayAccess` consumption), `src/optimize/fold.rs` (export `try_fold_array_access` to the propagate module — add `pub(super) use ops::try_fold_array_access;` alongside the existing re-exports)
- Test: `src/optimize/tests/propagate/collections.rs`

**Interfaces:**
- `assigned_array_fact(value: &Expr) -> Option<Expr>` (in `env.rs` or `fold/scalar.rs`): `ArrayLiteral`/`ArrayLiteralAssoc` with ≤ 64 elements, every key and value a `scalar_value` literal → `Some(value.clone())`.
- `env_after_scalar_assign` ordering: apply RHS invalidation (Task 7), volatility guard, then `assigned_scalar_value` → `Scalar`, else `assigned_array_fact` → `ArrayLit`, else `Variable(src)` with an env fact → copy the fact (COW), else remove.
- `propagate_expr` `ArrayAccess` arm: after recursing into array/index, when array is `Variable(name)` with `ArrayLit(lit)` and `try_fold_array_access(lit, &index)` yields a kind → return `fold_expr(Expr { kind, span })`. Never substitute `ArrayLit` at plain `Variable` sites.

- [ ] Write failing tests: `$a = [1, 2, 3]; echo $a[1];` → `echo 2`; assoc `['k' => 7]` with `$a['k']` → `7`; `$b = $a; $b[0] = 9; echo $a[1];` still folds (COW) while `$b[1]` does not; `sort($a)` / `$r = &$a` / `foreach ($a as &$v)` / `unset($a)` each kill the fact; out-of-range `$a[9]` stays a runtime access; a 65-element literal carries no fact; `$a[$i]` with unknown `$i` stays; `f($a)` by-value keeps the fact for a no-by-ref user callee.
- [ ] Run → FAIL; implement; PASS; `cargo build` clean; commit `feat(optimize): propagate array-literal facts for heap-backed locals`.

### Task 10: End-to-end hazard tests

**Files:**
- Create: `tests/codegen/optimizer/memory_model_propagation.rs`
- Modify: `tests/codegen/optimizer/mod.rs` (or wherever sibling modules are registered — mirror `eir_constant_propagation.rs`)

**Interfaces:** `compile_and_run` fixtures asserting stdout, PHP-cross-checked (`php -r`) where behavior is subtle. Programs must defeat *earlier* passes where the point is runtime behavior — use `$argc`-guarded writes where needed.

- [ ] Add compile-and-run tests (one function each, inline PHP): array fact + element write ordering; COW copy divergence (`$b = $a; $b[0] = 9; echo $a[0], $b[0];`); `sort($a)` then element reads; by-ref user function mutating a caller local before an echo; `global`-writing callee invoked from top level between assign and echo; `foreach (… as &$v)` post-loop `$v` write observed through the array; closure `use (&$x)` invoked between assign and echo; pure user call between assign and echo (fold expected — assert output only).
- [ ] Run: `cargo test --test codegen_tests memory_model_propagation` → PASS (these lock semantics, so they should pass immediately; any failure is a Task 1–9 bug — fix before proceeding).
- [ ] Commit `test(optimize): end-to-end memory-model propagation hazard coverage`.

### Task 11: Docs + ROADMAP

**Files:**
- Modify: `docs/internals/the-optimizer.md` (describe facts/invalidation/volatility; drop the "richer memory-model-aware propagation across heap-backed locals" bullet from "What the optimizer does not do yet")
- Modify: `ROADMAP.md:943` → `[x]` with a one-line summary of what shipped (match sibling entries' style)

- [ ] Update both; `git diff --check`; commit `docs: memory-model-aware propagation internals and ROADMAP`.

## Plan self-review

- **Spec coverage:** design §1 → Tasks 6–7; §2 → Task 1; §3 → Tasks 2, 4, 5; §4 → Tasks 8–9; testing section → Tasks 1–10; docs → Task 11. No gaps.
- **Type consistency:** `Invalidation`, `ByRefSignatures`, `PropagatedValue`, `lvalue_root`, `with_function_scope` are each defined once and consumed under the same names in later tasks.
- **Ordering hazards:** Task 4 (mask) must precede Task 5 (map install) — enforced by numbering; Task 7 depends on 1, 2, 3, 6; Task 9 depends on 7 + 8.
