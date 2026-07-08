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
| `foreach ($a as &$v)` | `$v` **and** `$a` (`$v` keeps aliasing the last element after the loop) |
| closure `use (&$x)` | `$x` at closure-creation |
| `global $x;` | `$x` (aliases global storage; callees can write it) |
| `static $x;` | `$x` (recursive calls write the shared cell) |
| by-ref arg to user-defined callee | argument root (callee may retain the reference) |
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
