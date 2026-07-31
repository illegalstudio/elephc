# Guard reasoning v2 for dead-code elimination

> **ROADMAP item (v0.26.x):** Guard reasoning v2 for dead-code elimination —
> broader range reasoning and multi-variable facts beyond current strict-scalar,
> boolean, loose-comparison, and safe relational-complement guards.

## Context

AST dead-code elimination (`src/optimize/control/dce/`) already tracks
path-local guard facts in `GuardState` and uses them to prune nested `if` /
`elseif` / `switch` / `try` regions. Current coverage (shipped across DCE v1–v3
and later relational/loose slices) is:

| Family | Storage | What it proves today |
|---|---|---|
| Boolean / truthiness | `truthy_vars` / `falsy_vars` (+ bool-exact) | `$flag` / `!$flag`; composite `&&` / `||` / De Morgan via `condition_guards` |
| Strict-scalar exact | `exact_guards` | `$x === lit` → nested opposing strict / truthiness pruned; also feeds `switch ($x)` |
| Strict-scalar exclusion | `excluded_guards` | `$x !== lit` / false side of `$x === lit` → drop that literal case / nested exact match |
| Loose-comparison complement | `condition_guards` (AST-structural) | `$x == 0` ⇒ nested `$x != 0` false (same AST shape) |
| Safe relational complement | `condition_guards` (AST-structural, NaN-gated) | taken-true `$x > 10` ⇒ nested `$x <= 10` false; taken-false refuses the inverse |

Two structural ceilings remain, both named by the ROADMAP item and by
`docs/internals/the-optimizer.md` ("What the optimizer does not do yet"):

1. **No range / interval domain.** `$x > 10` only records the structural
   complement of that exact AST. It does **not** prove `$x > 5`, `$x >= 11`,
   `$x === 3` false, or that `case 0:` / `case 5:` are impossible in a later
   `switch ($x)`. Nested relations with different literals never meet.
2. **No cross-variable relational / equality atoms.** Multi-variable support
   today is boolean structure only (`$a && $b` as one `ConditionGuard`). Facts
   like `$x === $y`, `$x > $y`, or substituting an exact guard into the other
   side (`$x === 3` + `$y > $x` ⇒ `$y > 3`) are absent.

Both limits live entirely inside the existing `extend_guards` /
`known_condition_value` / `clear_guards_for_name` protocol. The pass stays
AST-local and path-cloned; there is no EIR guard lattice and this plan does
not introduce one.

Purity / may-throw v2 (shipped, ROADMAP checked) is an **eligibility gate**,
not a shared fact lattice: `record_condition_guard` only records when
`!has_side_effects && !may_throw`. Sharper effects already allow more
conditions into `condition_guards`; range / relational recording reuses that
same gate. Exception-aware DCE v2 (next unchecked sibling) is out of scope
here — it owns throw-type reachability and finally invalidation, not the
guard lattice itself.

## Goal

Extend `src/optimize/control/dce/` so that:

- integer relational guards accumulate into a per-variable **interval** fact,
  and nested relational / strict-int / switch-case queries can be answered from
  that interval (not only from exact AST complement match);
- pure relational / equality atoms between `Var|Lit` sides are recorded as
  first-class **relational facts**, with safe complements and exact-guard
  substitution into the other side;
- invalidation, switch pruning, and elseif cumulative-false prefixes consume
  the new domains through the same path-local protocol as today.

## Non-goals

- EIR port of the guard lattice (EIR already has const-fold / branch-simplify /
  DCE; the ROADMAP item is AST DCE).
- Loop-condition strengthening (`While` / `For` currently clone outer guards
  into the body and do not extend from the loop condition — leave that alone).
- General CFG join / meet of `GuardState` across arbitrary merges (still
  path-cloned; no new fixed-point over blocks).
- Float interval domain as a first-class lattice. Float relational complements
  keep today's NaN policy (`comparison_inverse_is_total`); do **not** build
  float intervals unless a side is proven finite / non-float by an exact guard.
- Proving PHP loose-equality coercion cross-facts (`$x == "0"` ⇒ `$x === 0`).
  Loose `==` / `!=` stay structural complements only.
- Exception-aware DCE v2, control-flow normalization v2, or tail-call
  optimization (separate ROADMAP items).

## Alternatives considered

- **EIR dominance-aware relational propagation:** would reuse `dominance` /
  `const_fold`, but PHP truthiness / loose equality / NaN totality are encoded
  in the AST DCE model today, and the ROADMAP item sits next to other AST
  optimizer bullets. Rejected for this slice.
- **Only widen `condition_guards` with more rewritten AST forms** (e.g. emit
  `$x > 5` whenever `$x > 10` is recorded): unbounded form explosion, no clean
  intersection across elseif false prefixes, weak switch integration. Rejected
  as the sole mechanism (complements stay for exact AST hits; ranges supersede
  transitive numeric cases).
- **Full relational abstract domain** (octagons / difference-bound matrices):
  far beyond the ROADMAP wording and the current Vec-backed `GuardState`.
  Rejected; keep atoms + intervals.
- **AST interval + relational atoms on `GuardState` (chosen):** smallest
  extension of the shipped protocol, mirrors how exact/excluded were added,
  reuses NaN totality helpers, and has a clear unit/e2e test mirror under
  `optimize/tests/dce/guards/` and
  `tests/codegen/optimizer/dead_code_elimination/guards/`.

## Design

### 1. Integer interval domain

```rust
/// Inclusive integer bounds; `None` means ±∞ on that side.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct IntInterval {
    pub(super) lo: Option<i64>, // Some(n) ⇒ x >= n
    pub(super) hi: Option<i64>, // Some(n) ⇒ x <= n
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct RangeGuard {
    pub(super) name: String,
    pub(super) interval: IntInterval,
}
```

Add `range_guards: Vec<RangeGuard>` to `GuardState`.

**Recording** (in `extend_guards`, after exact/excluded handling), when the
condition is a pure relational `BinOp` with one `Variable` side and one
`IntLiteral` side (either order), and the branch is taken **or** the inverse
is total for that op:

| Taken condition | Interval contribution |
|---|---|
| `$x > n` | `lo = n+1` (saturate / refuse on `i64::MAX`) |
| `$x >= n` | `lo = n` |
| `$x < n` | `hi = n-1` (refuse on `i64::MIN`) |
| `$x <= n` | `hi = n` |
| `$x > n` false, total inverse | treat as `$x <= n` |
| … | symmetric for the other three |

Intersect with any existing `RangeGuard` for that name (`lo = max`, `hi = min`).
Empty intersection (e.g. `lo > hi`) means the current branch is already
unreachable — callers can leave that to `known_condition_value` returning
`Some(false)` on the branch condition itself when queried later; do **not**
invent a new "bottom" control-flow rewrite in this slice beyond what DCE
already does when a condition is known.

**Exact-guard coupling:** recording `exact_guards` with `GuardLiteral::Int(n)`
also sets `range_guards` to `[n, n]`. Clearing a name clears its range fact.
An excluded int does **not** punch a hole in the interval (holes need a
different domain); exclusions continue to work through `excluded_guards`.

**Query** (in `known_condition_value_base`, after the strict-scalar arm):

- nested `$x <op> m` / `$x === m` / `$x !== m` answered from the interval when
  every value in the interval agrees on the result;
- truthiness of `$x` when the interval is entirely positive or entirely
  non-positive-with-zero-excluded is **out of scope** for v1 of this slice
  unless it falls out cheaply from `[lo, hi]` with `0 ∉ [lo, hi]` and no
  negative/positive split — prefer proving relational/strict-int only first.

**Switch:** in `classify_switch_patterns_*` / guarded case helpers, an int
`case lit:` whose literal lies outside the subject's `range_guards` interval
is impossible (same role excluded literals already play for single points).

**Float / string relational ops:** do not create `RangeGuard`s. Keep recording
them only as today's `condition_guards` complements when safe.

### 2. Relational / equality atoms (multi-variable)

```rust
#[derive(Clone, PartialEq, Eq)]
pub(super) enum RelSide {
    Var(String),
    Int(i64),
    // optional later: Exact reuse via GuardLiteral — start with Int + Var
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct RelationalGuard {
    pub(super) left: RelSide,
    pub(super) op: RelOp,       // thin enum: Lt/Le/Gt/Ge/StrictEq/StrictNotEq
    pub(super) right: RelSide,
    pub(super) holds: bool,     // recorded polarity
}
```

Add `relational_guards: Vec<RelationalGuard>` to `GuardState`.

**Recording gate:** both sides are `Variable` or `IntLiteral`; operator is
relational or `===` / `!==`; condition is pure / non-throwing (same as
`record_condition_guard`). Also record the safe complement via the existing
`inverse_comparison_op` + `comparison_inverse_is_total` rules (refuse
taken-false float-like cases; int relational inverses are total).

**Loose `==` / `!=` between two variables:** record as `condition_guards` only
(structural), not as `RelationalGuard`, to avoid inventing coercion theorems.

**Query:**

1. Structural hit against a recorded atom (and its complement), including
   operand-swapped forms (`$x > $y` ↔ `$y < $x`).
2. **Exact substitution:** if one side is a `Var` with an `exact_guards`
   `Int(n)` (or a `RangeGuard` collapsed to a point), rewrite the atom to
   `Var <op> n` / `n <op> Var` and discharge through the interval domain
   (or record a derived range on extend when the other side is a var).
3. On `extend_guards` for `$y > $x` when `$x` already has exact/range int
   facts, eagerly strengthen `$y`'s `RangeGuard` (and symmetrically). This is
   the multi-variable → range bridge the ROADMAP wording implies.

**Invalidation:** `clear_guards_for_name` drops every `RelationalGuard` that
mentions the name (either side), same as `condition_guards`.

### 3. Protocol integration (no new driver)

Keep the existing control flow:

```
extend_guards on branch entry
  → exact / excluded / condition_guards (unchanged)
  → NEW: intersect range_guards from int relational
  → NEW: record relational atoms + derive ranges via exact substitution
known_condition_value
  → condition_guards / boolean / exact / excluded (unchanged)
  → NEW: interval discharge for int relational / strict-int
  → NEW: relational atom hit + swapped forms + exact substitution
invalidate / clear_guards_for_name
  → NEW: clear range_guards + relational_guards by name
switch / elseif cumulative false prefixes
  → NEW: consume range_guards for int case labels
  → relational atoms participate automatically once known_condition_value does
```

`And` / `Or` weak-side conservatism stays as today (no disjunctive join).
Ranges and relational atoms are only extended on the strong side of those
connectives, matching exact/condition recording.

### 4. File layout

Prefer cohesion over a new top-level pass:

| File | Change |
|---|---|
| `src/optimize/control/dce/state.rs` | `IntInterval`, `RangeGuard`, `RelSide`, `RelOp`, `RelationalGuard`; fields on `GuardState` |
| `src/optimize/control/dce/guards/record.rs` | record + clear + `extend_guards` hooks; maybe split `range.rs` / `relational.rs` siblings under `guards/` if `record.rs` would mix too many responsibilities |
| `src/optimize/control/dce/guards/eval.rs` | query discharge in `known_condition_value_base` |
| `src/optimize/control/dce/guards.rs` | re-exports if modules split |
| `src/optimize/control/dce/switches.rs` (and helpers) | int case impossible-under-range |
| Tests | new modules under existing `guards/` trees (see below) |

Split `guards/range.rs` + `guards/relational.rs` only when `record.rs` /
`eval.rs` would otherwise mix unrelated domains past the file-size /
responsibility guideline. First implementation may land helpers at the bottom
of `record.rs` / `eval.rs` and split in a follow-up commit inside the same PR
if churn is high.

## Testing strategy

Mirror the v1 layout: AST unit tests first, then codegen e2e that also assert
dead string literals are absent from generated assembly where useful
(`dead-range` / `dead-relvar` markers, same pattern as `dead-loose` /
`dead-rel`).

### Unit — `src/optimize/tests/dce/guards/`

New files (or clearly named cases in a new `range_guards.rs` /
`relational_guards.rs` module pair):

1. **Range intersect / nested relational:**
   `if ($x > 10) { if ($x > 5) { keep } else { dead } }` → else pruned;
   `if ($x > 10) { if ($x <= 10) { dead } }` still works (regression).
2. **Range vs strict int:**
   `if ($x >= 0 && $x <= 0) { /* equiv exact 0 via intersect */ if ($x === 1) dead }`
   Prefer two nested guards if `&&` strong-side recording is enough:
   `if ($x >= 0) { if ($x <= 0) { if ($x === 1) dead; if ($x === 0) keep } }`.
3. **Elseif false prefix widens exclusion via range:**
   `if ($x < 0) {…} elseif ($x > 0) {…} else { /* x == 0 */ if ($x === 1) dead }`.
4. **Switch int cases outside range:**
   `if ($x > 5) { switch ($x) { case 0: dead; case 6: keep; } }`.
5. **Overflow refusal:** `$x > i64::MAX` recording must not wrap; no prune from
   a bogus interval (fixture with `IntLiteral(i64::MAX)`).
6. **Multi-var equality:**
   `if ($x === $y) { if ($x !== $y) dead else keep }`.
7. **Multi-var relational + exact substitution:**
   `if ($x === 3) { if ($y > $x) { if ($y > 3) keep; if ($y <= 3) dead } }`.
8. **Write invalidation:** assign to `$x` kills range + relational facts that
   mention `$x`; unrelated `$y` facts remain.
9. **NaN / float refusal regression:** taken-false `$f > 1.0` still must not
   assert `$f <= 1.0`; int path unchanged.
10. **Impure / throwing condition:** call in condition still records nothing
    new (purity gate).

### E2E — `tests/codegen/optimizer/dead_code_elimination/guards/`

Add `range_guards.rs` and `relational_guards.rs` (register in the guards
`mod.rs`) with `compile_and_run` programs using runtime-unknown inputs
(`$argc` / `$argv`) so AST constant folding does not erase the construct
before DCE. Assert stdout and, for at least one range and one relational
case, that a uniquely named dead marker string does not appear in the
generated assembly.

PHP cross-check (`ELEPHC_PHP_CHECK=1`) on the e2e fixtures that have pure PHP
surface — optional but preferred for relational edge cases.

## Documentation / roadmap / changelog

When implementation lands (not in the plan-only PR):

- `docs/internals/the-optimizer.md` — document range + relational atoms under
  Pass 5; remove the matching bullet from "What the optimizer does not do yet".
- `ROADMAP.md` — mark the Guard reasoning v2 item `[x]` with a one-line
  summary in the existing style.
- `CHANGELOG.md` — one terse `[Unreleased]` bullet
  (e.g. "DCE guard reasoning understands integer ranges and cross-variable
  relational facts").
- No new `examples/` program required unless an existing optimizer example
  naturally showcases it; this is an optimizer refinement, not a new language
  construct. Skip example creation unless a tiny readable demo fits an
  existing folder.

## Implementation tasks

### Task 1: `IntInterval` + `RangeGuard` on `GuardState`

**Files:**
- Modify: `src/optimize/control/dce/state.rs`
- Modify: `src/optimize/control/dce/guards/record.rs` (`clear_guards_for_name`)

**Interfaces:**

```rust
pub(super) struct IntInterval { lo: Option<i64>, hi: Option<i64> }
impl IntInterval {
    fn unconstrained() -> Self;
    fn point(n: i64) -> Self;
    fn intersect(self, other: Self) -> Option<Self>; // None ⇒ empty
    fn contains(self, n: i64) -> bool;
}
pub(super) struct RangeGuard { name: String, interval: IntInterval }
// GuardState gains range_guards: Vec<RangeGuard>
```

- [ ] Add types + default field; clear by name in `clear_guards_for_name`.
- [ ] `cargo build` clean; commit `feat(optimize): add integer range facts to DCE GuardState`.

### Task 2: Record int relational ranges in `extend_guards`

**Files:**
- Modify: `src/optimize/control/dce/guards/record.rs` (or new `guards/range.rs`)
- Modify: `src/optimize/control/dce/guards/eval.rs` if a shared matcher helps
- Test: `src/optimize/tests/dce/guards/range_guards.rs` (new)

**Interfaces:**

```rust
fn int_relational_guard(condition: &Expr) -> Option<(&str, BinOp, i64)>;
fn interval_from_relational(op: BinOp, n: i64, branch_taken: bool) -> Option<IntInterval>;
fn record_range_guard(guards: &mut GuardState, name: &str, contrib: IntInterval);
// exact int recording also sets point interval
```

- [ ] Write failing unit tests for nested `$x > 10` / `$x > 5` and `$x > 10` /
  `$x <= 10` (the latter already passes via complements — keep as regression).
- [ ] Run → FAIL on the transitive case; implement recording + point coupling
  from exact int; PASS; commit `feat(optimize): record integer range guards from relational branches`.

### Task 3: Discharge ranges in `known_condition_value`

**Files:**
- Modify: `src/optimize/control/dce/guards/eval.rs`
- Test: extend `range_guards.rs`

**Interfaces:**

```rust
fn known_from_range(guards: &GuardState, condition: &Expr) -> Option<bool>;
// called from known_condition_value_base after strict_scalar handling
```

Prove nested int relational and `$x === n` / `$x !== n` when the interval
entails a unique boolean. Do not prove loose `==`.

- [ ] Failing tests for `$x >= 0` then `$x <= 0` then `$x === 1` dead /
  `$x === 0` keep; overflow refusal fixture.
- [ ] Implement; PASS; commit `feat(optimize): prove nested conditions from integer range guards`.

### Task 4: Switch + elseif consumption of ranges

**Files:**
- Modify: `src/optimize/control/dce/switches.rs` (and any
  `classify_switch_patterns_*` helpers)
- Modify: `src/optimize/control/dce/ifs.rs` only if cumulative false prefixes
  need an explicit range intersect beyond `extend_guards(..., false)`
- Test: unit range tests for switch / elseif; mirror patterns in
  `excluded_guards.rs` / `elseif_suffixes.rs`

- [ ] Failing tests: outer `$x > 5` drops `case 0:`; elseif chain else sees
  `$x === 0`-equivalent via `< 0` false ∩ `> 0` false (if false-branch
  relational recording from Task 2 already intersects, assert the nested
  prune; otherwise extend false-branch interval recording carefully with
  totality rules).
- [ ] Implement the minimum switch hook; PASS; commit `feat(optimize): prune switch int cases outside known ranges`.

### Task 5: Relational atoms on `GuardState`

**Files:**
- Modify: `src/optimize/control/dce/state.rs`
- Modify: `src/optimize/control/dce/guards/record.rs` (or `guards/relational.rs`)
- Test: `src/optimize/tests/dce/guards/relational_guards.rs` (new)

**Interfaces:**

```rust
enum RelSide { Var(String), Int(i64) }
enum RelOp { Lt, Le, Gt, Ge, StrictEq, StrictNotEq }
struct RelationalGuard { left: RelSide, op: RelOp, right: RelSide, holds: bool }
fn relational_atom(condition: &Expr) -> Option<(RelSide, RelOp, RelSide)>;
fn record_relational_guard(...);
fn swap_rel(op: RelOp) -> RelOp; // for operand swap normalization
```

- [ ] Failing test: `$x === $y` then `$x !== $y` pruned; write to `$x`
  invalidates; impure condition ignored.
- [ ] Implement record/clear/safe complement; PASS; commit `feat(optimize): record cross-variable relational guard atoms`.

### Task 6: Query atoms + exact/range substitution bridge

**Files:**
- Modify: `src/optimize/control/dce/guards/eval.rs`, `record.rs`
- Test: extend `relational_guards.rs`

- [ ] Failing test: `$x === 3` then `$y > $x` then nested `$y <= 3` dead /
  `$y > 3` keep (requires either query-time substitution or eager range
  strengthen on extend — prefer eager strengthen in `extend_guards` when the
  other side becomes a concrete int interval, plus query-time structural /
  swapped hits).
- [ ] Implement; PASS; commit `feat(optimize): discharge relational guards via exact and range substitution`.

### Task 7: Codegen e2e mirrors

**Files:**
- Create: `tests/codegen/optimizer/dead_code_elimination/guards/range_guards.rs`
- Create: `tests/codegen/optimizer/dead_code_elimination/guards/relational_guards.rs`
- Modify: guards `mod.rs` to register them

- [ ] Port the highest-signal unit fixtures to `compile_and_run` with `$argc`
  opacity; assert stdout + dead-marker assembly absence for one range and one
  relational case.
- [ ] `cargo test --test codegen_tests range_guards` and
  `… relational_guards` PASS; commit `test(optimize): e2e coverage for guard reasoning v2`.

### Task 8: Docs + ROADMAP + CHANGELOG

**Files:**
- Modify: `docs/internals/the-optimizer.md`
- Modify: `ROADMAP.md` (Guard reasoning v2 → `[x]`)
- Modify: `CHANGELOG.md` (`[Unreleased]`)

- [ ] Update; `git diff --check`; commit `docs: guard reasoning v2 internals and ROADMAP`.

## Focused verification (per implementation PR)

Do **not** run the full suite locally by default. Per commit / before push:

```bash
cargo build
cargo test --lib optimize::tests::dce::guards
cargo test --test codegen_tests dead_code_elimination::guards
git diff --check
```

Widen only if a shared helper (`cfg.rs`, effect summaries) is touched.

## Plan self-review

- **Spec coverage:** Design §1 → Tasks 1–4; §2 → Tasks 5–6; §3 protocol →
  Tasks 2–6; testing → Tasks 2–7; docs → Task 8. Matches ROADMAP wording
  (range + multi-variable) and the optimizer "not yet" bullet.
- **Type consistency:** `IntInterval` / `RangeGuard` / `RelSide` / `RelOp` /
  `RelationalGuard` defined once on `state.rs` and consumed under those names.
- **Ordering hazards:** Task 3 depends on 2; Task 4 depends on 3 (switch
  queries need discharge); Task 6 depends on 2+5 (substitution into ranges);
  Task 7 after unit green; Task 8 last.
- **Soundness anchors:** reuse `comparison_inverse_is_total` for false-branch
  complements; refuse `i64` overflow when shifting bounds; never invent float
  intervals; invalidate on any write to a mentioned name; keep impure /
  may-throw conditions unrecorded.
- **Sibling isolation:** does not implement Exception-aware DCE v2 or
  Control-flow normalization v2; does not require EIR changes.
- **PR hygiene:** no open PR currently implements this item (checked against
  open/draft PRs and `guard reasoning` / DCE searches; historical #87/#88 are
  earlier DCE slices; #631 is purity/may-throw v2).
