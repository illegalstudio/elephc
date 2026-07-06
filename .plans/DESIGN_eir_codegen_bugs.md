# Design: EIR Lowering / Codegen PHP-Compatibility Bug Fixes (Issues #384, #340, #377, #381, #360)

Status: **Design only — no implementation.** All designs are grounded in the current
`main` source. Each issue was reproduced locally (see "Reproduction" notes) except
#340 and #377, whose original symptom has already been fixed on `main` by commits
`9b5913988` (interpolation) and `00aecd00f` (release overwritten locals by storage
type); for those two the design targets the **residual gap** that still lets
related inputs go wrong, plus hardening tests so the regression cannot return.

Active-backend constraints respected throughout:
- No edits to `src/codegen/expr/`, `src/codegen/stmt/`, `src/codegen/builtins/`
  (frozen legacy). New semantics go through `src/ir_lower/` and
  `src/codegen_ir/`.
- ARM64 **and** x86_64 lowerings for every new codegen path.
- Every new `emitter.instruction(...)` carries a `//` comment at column 81.
- New/edited Rust files get a `//!` module preamble and `///` docblocks on every
  function. Leaf files stay under the 500-LOC cohesion guideline.
- Runtime cache (`~/.cache/elephc/*.o`) must be invalidated after any runtime
  emitter change; the design notes this where relevant.

---

## Issue #384 — `match` subject call loses by-reference write-back

### Reproduction
```
function bump(&$i) { $i++; return $i; }
$i = 0;
echo match (bump($i)) { 1 => 'one', default => 'other' } . '|' . $i;
```
PHP prints `one|1`. elephc prints `one|0`.

### Root cause (verified)
The bug is **not match-specific**. The identical program with
`echo bump($i) . '|' . $i;` also prints `1|0`, and `switch (bump($i))` prints
`one|1`. The difference is statement vs. expression scope.

`--emit-ir` (with and without `--no-ir-opt`) for both the bare call and the
`match` form shows the second read of `$i` lowered as
`v6: I64 = const_i64 0` — i.e. **the AST-level constant propagator already
folded `$i` to `0` before EIR lowering**. The generated assembly
(`/tmp/t384b.s:4073`) correctly passes `sub x0, x29, #120` (the address of
`$i`'s slot) to `_fn_bump`, and `_fn_bump` correctly does
`ldr x0, [x9]` / `str x0, [x9]` through that pointer — so the *runtime*
write-back works. The folded `0` for the later read simply never observes it.

The folding happens in `src/optimize/propagate/expr.rs:52`:

```rust
ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
    left: Box::new(propagate_expr(*left, env)),
    op,
    right: Box::new(propagate_expr(*right, env)),   // <-- same env
},
```

The right operand is propagated with the **same `env`** as the left. A
`FunctionCall` left operand (`bump($i)`) returns `None` from
`expr_local_writes` ("may write anything"), but that write-set is consulted
only at **statement** boundaries (`stmt_local_writes`). Inside one expression
the propagator never invalidates variables that the left subexpression may
have mutated. `ExprKind::Match` (`propagate_expr` at line 111) propagates the
subject and each arm with the same `env` too, so the arms see the pre-subject
value of `$i`.

`switch` works only because the `echo '|' . $i;` is a *separate statement*
after the switch: `stmt_local_writes(switch_stmt)` returns `None` (the
subject contains a call), so the next statement starts with a fully
invalidated env.

### Spec
- **PHP behavior**: By-reference parameter mutation is a visible side effect
  on the caller's variable. Any subsequent read of that variable — in the
  same expression or a later statement — must observe the mutated value.
  Evaluation order is left-to-right for `BinaryOp`, subject-before-arms for
  `match`, condition-before-body for `??`/`?:`, callee-before-args for calls.
- **elephc current**: Constant propagation within a single expression reuses
  one `env` across all subexpressions, so a by-ref call followed by a read of
  the mutated variable in the same expression folds to the stale constant.
- **Fix target**: Intra-expression propagation must invalidate every variable
  that a *may-write* subexpression can mutate, *before* propagating the next
  subexpression in source/evaluation order. This is the general fix; `match`
  is one beneficiary.

### Architecture

**Primary file**: `src/optimize/propagate/expr.rs` (rewrite of
`propagate_expr` to thread a *write-invalidate* accumulator through
evaluation order).

New helper in the same file (or a small new `src/optimize/propagate/order.rs`
if `expr.rs` would exceed the soft 500-LOC limit after the change — current
`expr.rs` is 439 LOC, so an in-place edit is preferred):

```rust
/// Returns the set of locals a subexpression may write, or `None` when it
/// may write anything (conservative). Mirrors `expr_local_writes` but is
/// kept in sync with the order in which `propagate_expr` visits children.
fn expr_may_write_locals(expr: &Expr) -> Option<HashSet<String>>
```

`propagate_expr` gains an internal recursive form that threads a mutable
`ConstantEnv`:

```rust
fn propagate_expr_ordered(expr: Expr, env: &mut ConstantEnv) -> Expr
```

For each composite variant, after propagating the left/first child with the
current `env`, compute `expr_may_write_locals(child)`:
- `Some(set)` → remove every name in `set` from `env` before propagating the
  next child.
- `None` → clear `env` entirely (unknown writes) before propagating the next
  child.

Variants that must invalidate between children, in evaluation order:
- `BinaryOp { left, op, right }` — invalidate after `left`, before `right`.
  (PHP evaluates `left` then `right` for all binary ops including `&&`/`||`,
  which already short-circuit in `propagate_expr` via their own arms — those
  keep their existing handling and just additionally invalidate.)
- `Match { subject, arms, default }` — invalidate after `subject`; each arm
  body is propagated in its own fresh env (arms are mutually exclusive, so a
  write in arm 1 must not be visible to arm 2).
- `Assignment { value, result_target, prelude, conditional_value_temp, .. }`
  — the `prelude` (by-ref setup) and `value` already mutate the target; keep
  existing handling but invalidate the target name after the value is
  propagated, before any `result_target` read.
- `NullCoalesce { value, default }`, `Ternary { .. }`,
  `ShortTernary { .. }` — invalidate after the condition/value, before the
  alternative (the alternative only runs when the first didn't, but
  conservatively the first may have written before deciding).
- `Pipe { value, callable }` — invalidate after `value`.
- `PreIncrement`/`PostIncrement`/`PreDecrement`/`PostDecrement` (as
  expression operands) — invalidate the named variable after the operand,
  before any surrounding continuation. (These already propagate as their own
  variant; the change is that a *parent* binary/match invalidates after them.)
- Function/method/constructor call variants (`FunctionCall`, `MethodCall`,
  `NullsafeMethodCall`, `StaticMethodCall`, `ExprCall`, `ClosureCall`,
  `NewObject`, `NewDynamic`, `NewDynamicObject`, `NewScopedObject`) — these
  already return `None` from `expr_local_writes`; the ordered propagator
  treats them as `None` writers and clears the env for any following sibling.

**Optimizer effect modeling** (`src/optimize/effects.rs` and
`src/optimize/effects/calls.rs`): no change required — `function_call_effect`
already sets `with_side_effects()`, which is what makes
`expr_local_writes` return `None`. The fix only changes *propagation*, not
*effect modeling*.

**Tests**: `src/optimize/tests/propagate/straight_line.rs` and
`src/optimize/tests/propagate/loops/foreach_loops.rs` get new unit tests
asserting the AST after propagation keeps the second read as a `Variable`
(not a literal) when a preceding call may write it. Codegen regression test
in `tests/codegen/callables/language_features.rs` (or a new
`tests/codegen/by_ref/intra_expr_writeback.rs`) with the issue's exact
program plus the bare-call and switch variants.

### Test plan
| # | PHP source | Expected | Covers |
|---|---|---|---|
| 1 | `function bump(&$i){$i++;return $i;} $i=0; echo match(bump($i)){1=>'one',default=>'other'}.'|'.$i;` | `one\|1` | Issue #384 exact |
| 2 | `function bump(&$i){$i++;return $i;} $i=0; echo bump($i).'|'.$i;` | `1\|1` | Bare call (same root cause) |
| 3 | `function bump(&$i){$i++;return $i;} $i=0; switch(bump($i)){case 1: echo 'one'; break; default: echo 'other';} echo '|'.$i;` | `one\|1` | Switch (must keep working) |
| 4 | `function add(&$a,$v){$a[]=$v;return count($a);} $a=[]; echo add($a,1).$a[0];` | `11` | Array append by-ref + read in same expr |
| 5 | `$i=0; echo ($i++).'|'.$i;` | `0\|1` | Post-increment in same expression |
| 6 | `function b(&$x){$x=5;return $x;} $i=0; $r = b($i) + $i; echo $r;` | `10` | Binary op + by-ref |
| 7 | `function b(&$x){$x=5;return 1;} $i=0; echo match(b($i)){1=>$i,default=>0};` | `5` | Match arm reads mutated var |
| 8 | `$i=5; echo $i . ($i=0) . $i;` | `500` | Assignment mid-expression (PHP eval order) |

Edge: `echo bump($i) . bump($i);` with `$i=0` → PHP: `12` (first call makes
`$i=1` returns `1`; second makes `$i=2` returns `2`). Validates that repeated
calls in the same expression each invalidate.

### Risk assessment
- **High-value risk**: over-invalidation could suppress *legitimate* folding
  and regress optimized output size/speed. Mitigation: the invalidation only
  fires for `None`/non-empty write-sets; pure subexpressions (literals,
  variable reads, pure builtins) return `Some(empty)` and don't invalidate.
- **Short-circuit ops**: `&&`/`||`/`??` only evaluate the right side
  conditionally; invalidating before the right side is still safe (the right
  side runs only when the left didn't write, so clearing is conservative, not
  wrong). Verify with `$i=1; $r = ($i=0) && ($i=2); echo $i;` → PHP `0`.
- **Regression watch**: existing propagation tests in
  `src/optimize/tests/propagate/**` and `tests/codegen/optimizer/**` must stay
  green. The `--no-ir-opt` IR for the issue program must show the second `$i`
  read as `load_local`, not `const_i64`.

---

## Issue #340 — String interpolation with array access emits corrupted output

### Reproduction
The issue's exact input `echo "{$a['x']}";` now compiles and prints `ok` on
`main` (fixed by `9b5913988 fix(lexer): support complex and simple string
interpolation forms`). Residual gaps found while grounding this design:

1. **Deprecated `${...}` form** (PHP 8.2 deprecation, still functional):
   `echo "${a['x']}";` → PHP prints `ok` (with deprecation notice); elephc
   emits the literal text `${a['x']}` (the `${` form is never recognized —
   `src/lexer/literals/strings.rs:140` only matches `{$`).
2. **Heredoc with a `}` inside a quoted key**: works for `"{$a['x']}"` but
   the brace/quote scanner in `capture_braced_expr` (line 289) is hand-rolled;
   it does not understand PHP string *escapes* inside the embedded string
   (only raw `\\` and matching quote). A key like `"{$a['x\'']}"` would
   confuse the depth counter. PHP rejects this as a parse error, so elephc
   should too — but it currently mis-captures rather than erroring cleanly.

The original `{{['x']}` symptom was the `{$a['x']}` form before
`9b5913988`; the design below locks in the fix and covers the residual
`${...}` and escape-balancing gaps.

### Spec
- **PHP behavior**: Inside a double-quoted string or heredoc body:
  - `{$expr}` — complex interpolation; `$expr` is any PHP expression that
    *starts with `$`* (a variable, `$this`, `${...}`, or a callable/array
    access chain on one). The braces close at the matching `}`, with
    balanced nested braces and string literals skipped verbatim.
  - `${name}` (deprecated since PHP 8.2) — simple variable lookup of
    `$name`; PHP emits `E_DEPRECATED` and treats it as `$name` (not an
    arbitrary expression; `${a['x']}` is special: PHP reads it as
    `$a['x']` for legacy compatibility — see PHP's `ZEND_COMPILE_*`
    `${...}` rule).
- **elephc current**: `{$expr}` is handled by `capture_braced_expr` +
  `tokenize_fragment` (re-lexes the captured text as `<?php $expr`); this
  works for `{$a['x']}`, `{$a[0]}`, `{$a['x']['y']}`, `{$obj->prop}`.
  `${name}` is not recognized at all (emitted literally).
- **Fix target**:
  1. Keep `{$expr}` handling as-is (regression-protected by new tests).
  2. Add `${name}` (deprecated) handling: recognize `${` at the start of an
     interpolation, emit a deprecation warning through the existing warning
     channel (`src/types/warnings/`), and parse the contents as a variable
     name with optional single-level `[offset]` (matching PHP's legacy
     `${a['x']}` semantics, which is *not* a full expression — only
     `$var` or `$var[offset]`).
  3. Harden `capture_braced_expr` to reject unbalanced/unterminated inputs
     with the existing `"Unterminated complex interpolation '{$...}'"` error
     rather than silently mis-capturing.

### Architecture

**Primary file**: `src/lexer/literals/strings.rs`.

- In `interpolate` (line 112), add a new arm before the `Some('$') =>` arm:

  ```rust
  Some('$') if input.peek_nth(1) == Some('{') => {
      input.advance_escape(); // consume '$'
      input.advance_escape(); // consume '{'
      let inner = capture_dollar_brace_var(input, span)?;
      has_interpolation = true;
      // emit a deprecation warning via the lexer's warning sink
      // ... build token stream for `$var` or `$var[offset]` ...
      push_interp_part(&mut tokens, &mut current, part, span);
  }
  ```

- New function `capture_dollar_brace_var` in the same file: parses only
  `$name`, `$name[offset]` (with `$name`, integer, or bareword offset —
  reusing `append_simple_offset_key`), and the closing `}`. Anything more
  complex (e.g. `${a->prop}`, `${func()}`) is a PHP parse error; emit a
  `CompileError` with a clear message rather than mis-capturing. This keeps
  the new code under the 500-LOC guideline (the file is currently ~722 LOC
  and a *single cohesive feature* — string scanning — so it already lives
  above the soft limit legitimately; adding ~40 LOC stays within the
  "cohesive leaf" exception).

- Deprecation warning: the lexer does not currently emit warnings; the
  existing `CompileError` is error-only. Wire a `DiagnosticSink`-style
  warning through the lexer's return path, or defer the warning to the type
  checker by tagging the token with a `DeprecatedInterpolation` marker.
  **Preferred minimal approach**: emit the warning during
  `src/types/warnings/expr_reads.rs` (which already walks interpolation
  expression nodes) by detecting a new `ExprKind::DeprecatedDollarBrace`
  wrapper. Add that wrapper in `src/parser/ast/expr.rs`, parse it in
  `src/parser/expr/prefix_complex.rs`'s interpolation-token consumer, and
  lower it in `src/ir_lower/expr/mod.rs` as the underlying `$var[offset]`.

- `capture_braced_expr` hardening: after the capture loop, if `depth != 0`
  is already caught (EOF → error). Add an additional check: if the captured
  `inner` re-lex fails (`tokenize_fragment` returns an error), propagate the
  error instead of pushing a malformed part.

**Parser**: `src/parser/expr/prefix_complex.rs` — the interpolation token
stream consumer already handles `Variable` + `ArrayAccess`; the new
`DeprecatedDollarBrace` node is a single new variant consumed there.

**EIR lowering**: `src/ir_lower/expr/mod.rs:90` dispatch — add
`ExprKind::DeprecatedDollarBrace { .. } => lower_deprecated_dollar_brace(...)`
that simply delegates to `lower_array_access` / `lower_expr(Variable)`.
No new EIR op needed.

**No codegen change** — interpolation lowers to normal `Op::StrInterpolate`
plus the inner expression's ops, which already work.

### Test plan
| # | PHP source | Expected | Covers |
|---|---|---|---|
| 1 | `$a=['x'=>'ok']; echo "{$a['x']}";` | `ok` | Original issue (regression lock) |
| 2 | `$a=['x'=>'ok']; echo "{$a["x"]}";` | `ok` | Double-quoted key inside braces |
| 3 | `$a=[['x'=>'deep']]; echo "{$a[0]['x']}";` | `deep` | Nested array access |
| 4 | `$a=['x'=>'ok']; echo "${a['x']}";` | `ok` (+ deprecation warning) | `${...}` legacy form |
| 5 | `$a=['k'=>1]; echo "${a['k']}";` | `1` | `${...}` int value |
| 6 | `$a=['x'=>'ok']; echo "pre {$a['x']} post";` | `pre ok post` | Interpolation with surrounding text |
| 7 | `$a=['x'=>'ok']; echo "{$a['x']}{$a['x']}";` | `okok` | Two interpolations in one string |
| 8 | `echo "{$a['x']}";` with `$a` undefined | PHP: empty + notice | Undefined-key interpolation (elephc may fatal; document) |
| 9 | Lexer error test: `echo "{$a['x']";` (unbalanced) | `Unterminated complex interpolation` | Hardened error path |

Error tests go in `tests/error_tests/` (new `interpolation.rs`); codegen
tests extend `tests/codegen/strings/interpolation_and_hashes.rs`.

### Risk assessment
- **Deprecation warning plumbing** is the most invasive part. If wiring a
  warning through the lexer is too costly, the minimal alternative is to
  *accept* `${name}` silently (matching the *value* PHP produces) and defer
  the deprecation notice to a follow-up. This still fixes the corruption
  (literal `${a['x']}` output) without the warning sink.
- **Parse-error parity**: PHP's `${...}` accepts only a narrow subset; any
  deviation must produce a clean error, not silent mis-capture. New error
  tests (#9) guard this.
- **Regression watch**: existing interpolation tests in
  `tests/codegen/strings/interpolation_and_hashes.rs` and
  `tests/parser_tests/**` must stay green.

---

## Issue #377 — Numeric loop variable that becomes float leaks heap

### Reproduction
On current `main` the exact issue program (`$i = 0; $i = $i + 1.0` for 2M
iterations) runs to completion with the correct result `1999999000000` in
both debug and release (verified locally). Commit `00aecd00f fix(ir): release
overwritten locals by storage type` closed the leak for the common path.

**Residual gap verified during grounding**: the slot-widening logic in
`src/ir/builder.rs:385 widened_local_storage_type` promotes
`Int → Float` storage to `Mixed` (boxed). The cleanup load in
`src/ir_lower/context.rs:594 release_stored_local_value` then loads the slot
as `Heap(Mixed)` and calls `Op::Release` even on the very first reassignment,
when the slot still holds a *raw unboxed int* (the `store_local v1 slot[1]`
in the loop preheader stores an `I64` into the not-yet-widened slot). The
runtime `__rt_decref_mixed` (in
`src/codegen/runtime/arrays/decref_mixed.rs:42`) treats the raw int bits as
a "pointer" and skips the decref because `0`/small ints are below
`_heap_buf`. This is *safe* (no crash) but means a raw-int slot can be
released as Mixed without effect, while a subsequently-stored boxed float
*is* released correctly on iteration 2+. The net leak on `main` is at most
one cell per widening transition — small, but it generalizes to other
type-widening transitions (`Bool → Float`, `Int → Str`, etc.) where the
first reassignment releases a non-boxed old value as if it were boxed.

### Spec
- **PHP behavior**: Reassigning a variable that changes its runtime type
  (int → float, int → string, etc.) must free the previous value's storage
  exactly once. No leak across long loops.
- **elephc current**: `release_stored_local_value` uses the slot's
  *widened storage type* (`local_php_type(slot)`) to load and release the
  previous occupant. When the previous occupant was stored before the
  slot was widened (raw int/bool), the release is a no-op for that
  transition; for transitions into refcounted storage it can leak the first
  old value.
- **Fix target**: Release the previous occupant using the *type the slot
  held when the previous store happened*, not the post-widening storage
  type. Concretely: track the previous logical type (`previous_type`
  captured at `store_local:617`) and use it to decide whether the old value
  is refcounted *before* the widening. If `previous_type` is non-refcounted
  (`Int`/`Bool`/`Float`/`Void`), skip the release entirely — there is no
  heap cell to free.

### Architecture

**Primary file**: `src/ir_lower/context.rs`.

Change `release_stored_local_value` (line 594) to take the *previous logical
type* as an argument, and skip the release when that type does not need
lifetime tracking:

```rust
fn release_stored_local_value_with_previous_type(
    &mut self,
    name: &str,
    slot: LocalSlotId,
    previous_type: PhpType,
    span: Option<Span>,
) {
    if !Ownership::php_type_needs_lifetime_tracking(&previous_type) {
        return;            // old occupant was a raw scalar; nothing to free
    }
    let previous = self.load_local_storage(name, slot, previous_type, span);
    crate::ir_lower::ownership::release_if_owned(self, previous, span);
}
```

Update the three call sites in `store_local` (lines 633, 642, 656) and the
epilogue path (line 948) to pass `previous_type` (or `local_php_type(slot)`
for the epilogue, which is correct there because the epilogue runs after all
stores have widened the slot to its final type and boxed every occupant).

**No EIR-op change**, no runtime change, no codegen change. This is purely a
lowering-side type-accuracy fix.

**Tests**: `src/ir_lower/tests/ownership.rs` — add a unit test that builds a
function with an `Int`-then-`Float` store to the same slot and asserts the
EIR contains exactly one `release` (for the boxed float on iteration 2+),
not a release of the raw int. End-to-end test in
`tests/codegen/runtime_gc/` (new `type_widening_loop.rs`) running the issue
program at 2M iterations with an assertion on the result and a *memory-debug*
run (`ELEPHC_HEAP_DEBUG=1`) asserting no leak is reported.

### Test plan
| # | PHP source | Expected | Covers |
|---|---|---|---|
| 1 | `$acc=0.0; for($i=0;$i<2000000;$i=$i+1.0){$acc=$acc+$i;} echo $acc;` | `1999999000000` | Issue #377 exact (2M, no OOM) |
| 2 | `$acc=0; for($i=0;$i<2000000;$i=$i+1){$acc+=$i;} echo $acc;` | `1999999000000` | Pure int loop (no widening, must not regress) |
| 3 | `$s=''; for($i=0;$i<100000;$i=$i+1){$s=$s.'x';} echo strlen($s);` | `100000` | Int→string reassignment, no leak |
| 4 | `for($i=0;$i<100000;$i++){ $x = ($i%2==0) ? 1 : 1.5; } echo $x;` | `1.5` | Alternating int/float reassignment |
| 5 | `$x=1; $x=2.0; $x=3; echo $x;` | `3` | Single int→float→int transition |
| 6 | `ELEPHC_HEAP_DEBUG=1` run of #1 | no leak report | Heap-debug instrumentation |

### Risk assessment
- **Risk**: skipping the release for a non-refcounted `previous_type` could
  *under*-free if a previous store actually boxed the value (e.g. a prior
  path stored a Mixed into the slot before this straight-line store). The
  `previous_type` captured at line 617 is the *logical* type from the last
  `set_local_type`, which is updated on every store, so it tracks what was
  actually stored. The epilogue path keeps the conservative storage-type
  release as a backstop.
- **Regression watch**: `tests/codegen/runtime_gc/**`, `tests/codegen/arrays/**`,
  and the existing ownership unit tests must stay green. Run with
  `ELEPHC_HEAP_DEBUG=1` locally on tests #1–#4.

---

## Issue #381 — `foreach` over arrays visits appended elements during value iteration

### Reproduction
```
$a = [1, 2];
foreach ($a as $v) { echo $v; if ($v === 1) $a[] = 3; }
echo '|' . count($a);
```
PHP: `12|3`. elephc: `123|3`.

### Root cause (verified)
`src/codegen_ir/lower_inst/iterators.rs:1183 lower_indexed_iter_next_aarch64`
(and the x86_64 twin at line 1202) re-reads the array length **every
iteration** from the live array header:

```
load_at_offset_scratch array_reg, [offset-0], "x9"   ; reload source pointer
load_at_offset        index_reg, [offset-8]          ; cursor
add                   index_reg, index_reg, #1
emit_load_from_address len_reg, array_reg, 0          ; <-- fresh length each iter
cmp                  index_reg, len_reg
```

PHP snapshots the array's length (and, for associative arrays, the entry
count) at the moment `foreach` begins iterating; appends during the loop
body do not extend the iteration. The current lowering treats the array as
live and re-reads its current length, so an append inside the body is
visible to the next `IterNext`.

The hash path (`__rt_hash_iter_next`, line 1225) is a runtime helper that
already advances a cursor against a snapshot taken at `IterStart` (the hash
iterator stores its own end pointer), so the hash case does **not** have this
bug — only the indexed-array fast path does.

### Spec
- **PHP behavior**: `foreach ($arr as $v)` captures the array's element count
  at loop entry. Appends (`$arr[] = …`), `$arr[count($arr)] = …`, and
  `array_push` inside the body do not add new elements to the iteration.
  Modifications to *existing* elements that change values are visible (PHP's
  semantics are nuanced, but the *count* is frozen). Unset of the current
  element is handled separately (PHP keeps a copy of the current value).
- **elephc current**: indexed-array `IterNext` re-reads the length each
  iteration → appends are visited.
- **Fix target**: `IterStart` for an indexed array must snapshot the array's
  initial length into the iterator state. `IterNext` compares the cursor
  against the **snapshotted** length, not the live array header.

### Architecture

**Primary file**: `src/codegen_ir/lower_inst/iterators.rs`.

1. **Add a length slot to the iterator state**:
   - In `src/codegen_ir/value_placement.rs`, bump
     `ITERATOR_STATE_BYTES` from `64` to `72` (one extra 8-byte word).
   - In `src/codegen_ir/lower_inst/iterators.rs`, add
     `const ITER_LENGTH_OFFSET_DELTA: usize = 64;` after
     `ITER_VALUE_ADDR_OFFSET_DELTA` (line 30). This slot is only used by
     the indexed-array path; the hash path keeps using
     `__rt_hash_iter_next` and ignores it.

2. **`lower_iter_start`** (line 48): for `IteratorSourceKind::Indexed { .. }`,
   after storing the source pointer and the initial cursor (`-1`), also load
   the array's current length (`emit_load_from_address(emitter, result_reg,
   source_reg, 0)`) and store it at `offset - ITER_LENGTH_OFFSET_DELTA`.
   This is the snapshot. ARM64 and x86_64 paths each get a 2-instruction
   addition with column-81 comments.

3. **`lower_indexed_iter_next_aarch64`** (line 1183) and
   `lower_indexed_iter_next_x86_64` (line 1202): replace the fresh length
   load with a load from the snapshot slot:
   - Remove: `emit_load_from_address(len_reg, array_reg, 0)`
   - Add: `load_at_offset(emitter, len_reg, offset - ITER_LENGTH_OFFSET_DELTA)`
   The `array_reg` load is still needed because the value loader
   (`load_current_array_value_*`) reads the element from `[array_reg,
   element_offset]`. Keep that load.

4. **Dynamic iterable/mixed indexed paths**
   (`initialize_dynamic_iterable_iterator`,
   `initialize_dynamic_mixed_iterator`): these already store the cursor;
   add the same length snapshot for the indexed branch. The hash and object
   branches leave the length slot uninitialized (the indexed `IterNext` is
   the only reader, and the dynamic dispatch routes hash/object to their own
   `IterNext` lowering, so an uninitialized length slot is never read on
   those paths).

5. **By-reference foreach**: PHP's `foreach ($a as &$v)` does **not** use the
   by-value length snapshot for indexed arrays; appended elements remain
   visible to the live iteration. Preserve that by making `IterNext` consult
   the `IterStart` by-ref flag and read the live array length on indexed by-ref
   paths. Verify with a regression test.

**No EIR-op change**, no `src/ir_lower/stmt/mod.rs` change. The EIR
`Op::IterStart`/`Op::IterNext` keep their existing operand lists; the
length snapshot is a codegen-internal detail of the indexed-array lowering.

**Runtime cache**: the iterator state size changes from 64 to 72 bytes. This
is a frame-layout change internal to each function; no runtime `.o` is
affected, so `~/.cache/elephc/*.o` does **not** need clearing. (Document
this in the PR description anyway.)

### Test plan
| # | PHP source | Expected | Covers |
|---|---|---|---|
| 1 | `$a=[1,2]; foreach($a as $v){echo $v; if($v===1)$a[]=3;} echo '|'.count($a);` | `12\|3` | Issue #381 exact |
| 2 | `$a=[1,2,3]; foreach($a as $v){echo $v; $a[]=9;} echo '|'.count($a);` | `123\|6` | Multiple appends |
| 3 | `$a=[1,2]; foreach($a as &$v){echo $v; if($v===1)$a[]=3;} echo '|'.count($a);` | `123\|3` | By-ref foreach live length |
| 4 | `$a=range(1,1000); $c=0; foreach($a as $v){$c++; $a[]=$v;} echo $c;` | `1000` | Large loop with appends, exact count |
| 5 | `$a=[1,2,3]; foreach($a as $k=>$v){echo "$k=$v,"; $a[]=$k;} echo count($a);` | `0=1,1=2,2=3,6` | Key+value foreach with appends |
| 6 | `$a=['a'=>1,'b'=>2]; foreach($a as $k=>$v){echo "$k=$v,"; $a['c']=3;} echo count($a);` | `a=1,b=2,3` | Hash foreach (already correct, lock in) |
| 7 | `$a=[1,2]; foreach($a as $v){echo $v; unset($a[0]);}` | PHP: `12` | Unset during foreach (current behavior, document) |

Codegen tests in `tests/codegen/arrays/foreach_snapshot.rs` (new file, with
module preamble).

### Risk assessment
- **Frame-size change** (64→72 bytes per iterator): the
  `allocates_iter_start_value_as_iterator_state` unit test in
  `src/codegen_ir/value_placement.rs` hard-codes the slot offset; update it.
  Any other test asserting iterator state size must be updated.
- **Uninitialized length slot** on hash/object paths: confirm via the
  validator that no `IterNext` on those paths reads
  `ITER_LENGTH_OFFSET_DELTA`. Add a debug assertion in
  `lower_indexed_iter_next_*` that the source kind is `Indexed` (already
  guaranteed by the dispatch in `lower_iter_next` line 124).
- **Regression watch**: all existing `tests/codegen/arrays/foreach*.rs` and
  `src/ir_lower/tests/arrays.rs` tests. Run the foreach key-write tests
  specifically (`tests/codegen/arrays/foreach_key_write.rs`) since they
  exercise the same iterator state layout.

---

## Issue #360 — Array elements cannot be passed to by-reference parameters

### Reproduction
```
function bump(&$x) { $x++; }
$a = [5];
bump($a[0]);
echo $a[0];
```
PHP: `6`. elephc: `parameter $x must be passed a variable` (compile error).

### Root cause (verified)
Two distinct gaps:

1. **Checker rejects valid lvalues**:
   - `src/types/checker/functions/call_validation.rs:332-347` rejects any
     by-ref argument that is not `ExprKind::Variable(_)`:
     ```rust
     if sig.ref_params.get(param_idx).copied().unwrap_or(false)
         && !matches!(arg.kind, ExprKind::Variable(_))
     { return Err("... must be passed a variable"); }
     ```
   - The same check is duplicated in
     `src/types/checker/functions/resolution/mod.rs:293-308` and `:527-542`
     for already-resolved callees.
   - PHP allows as by-ref targets: `$var`, `$a[idx]`, `$a[]` (append),
     `$a[$i]`, `$o->prop`, `$$var`, and `$a[idx][idx2]` (nested). PHP 8
     also accepts `$a[]` (append) as a write target for by-ref.

2. **EIR lowering has no by-ref array-element path**:
   - `src/ir_lower/expr/mod.rs:4315 lower_by_ref_array_arg_with_signature`
     only handles the *array-widening* case (converting `Array(Int)` to
     `Array(Mixed)` before a Mixed ref param). It does **not** pass the
     element's address.
   - For a plain `$a[0]` argument to a by-ref param, `lower_arg_with_signature`
     (line 4240) falls through to `lower_expr(arg)`, which emits an
     `Op::ArrayGet` (a *read*) and passes the value. The codegen
     `plan_ref_arg_writebacks` (`src/codegen_ir/lower_inst.rs:5194`) only
     plans writebacks for **Mixed-typed parameters** with scalar sources, so
     an `Int`-typed element passed to an `Int` by-ref param has no writeback
     and no address passing — even if the checker accepted it, the mutation
     would be lost.

### Spec
- **PHP behavior**: A by-reference parameter accepts any lvalue: a
  variable, an array element (`$a[0]`, `$a['k']`, `$a[$i]`, `$a[]`), a
  nested array element, or an object property. The callee mutates the
  *caller's storage*. For a packed array with unboxed scalar elements, the
  callee mutates the element slot in place; for a Mixed/assoc array, the
  callee mutates the boxed cell. `$a[]` as a by-ref target appends a new
  element (default `null`) and passes its address.
- **elephc current**: Checker rejects non-variable lvalues; lowering has no
  element-address path.
- **Fix target**:
  1. Checker accepts `ExprKind::ArrayAccess { array: Variable, .. }` (and
     nested) and `ExprKind::PropertyAccess { .. }` as by-ref targets, with
     the existing "must be passed a variable" error retained only for
     non-lvalue expressions (literals, calls, binary ops, constants).
  2. EIR lowering passes the *address* of the array element (or property
     slot) to the callee, and the codegen writeback path copies any boxed
     cell back into the array element after the call.

### Architecture

This is the largest of the five fixes. It splits into the checker change
(small, unblocks the error) and the lowering/codegen change (substantial,
actually makes the mutation work).

**Phase A — Checker (unblock the error)**

Files: `src/types/checker/functions/call_validation.rs`,
`src/types/checker/functions/resolution/mod.rs`.

Introduce a shared predicate so the three duplicated checks stay in lockstep
(per the AGENTS.md "do not maintain parallel tables" rule):

```rust
// in src/types/checker/functions/mod.rs or a new lvalues.rs
pub(super) fn is_by_ref_lvalue(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, .. } => is_by_ref_lvalue_base(array),
        ExprKind::PropertyAccess { object, .. } => is_by_ref_lvalue_base(object),
        ExprKind::DynamicPropertyAccess { object, .. } => is_by_ref_lvalue_base(object),
        _ => false,
    }
}
fn is_by_ref_lvalue_base(expr: &Expr) -> bool {
    matches!(expr.kind,
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. })
}
```

Replace the three `!matches!(arg.kind, ExprKind::Variable(_))` checks with
`!is_by_ref_lvalue(arg)`. The error message can stay "must be passed a
variable" (PHP's own message) or be widened to "must be passed a variable or
array element" — match PHP's wording in tests.

**Phase B — EIR lowering (make it work)**

The hard part: passing the address of `$a[0]`. Packed arrays in elephc store
unboxed scalar elements inline (`Array(Int)` → contiguous 8-byte slots);
Mixed/assoc arrays store boxed `Mixed` cells. The by-ref mechanism already
has two paths in `src/codegen_ir/lower_inst.rs`:
- `materialize_local_ref_arg_address` (line 5388): emits the *frame slot
  address* of a local variable. This is what makes `bump($i)` work for
  scalar by-ref params.
- `materialize_temporary_ref_arg_cell` (line 5278): allocates a heap
  ref-cell, copies the value in, passes the cell pointer, and
  `emit_ref_arg_writebacks` copies back.

For `$a[0]`:
- **Packed scalar array**: the element lives at
  `array_header + 16 + index * 8` (header is 16 bytes: length + capacity).
  The address is `array_ptr + 16 + index*8`. We can compute this and pass
  it directly — the callee reads/writes through that pointer exactly like
  it does for a local slot. No writeback needed because the write goes
  directly into the array storage.
- **Mixed/assoc array**: the element is a boxed `Mixed` cell allocated on
  the heap. The element's address is the cell pointer (read from the hash
  bucket). Pass that pointer; the callee mutates the cell in place. Again
  no writeback needed because the cell is shared.

So the design is: compute the *element address* and pass it, mirroring
`materialize_local_ref_arg_address` but with the address derived from the
array pointer + index instead of a frame slot.

New EIR op (so the codegen can emit target-specific address arithmetic
without the lowering having to know element byte sizes):

```rust
// src/ir/instr.rs
ArrayElementAddr,   // operands: [array_ptr, index]; result: I64 pointer
PropSlotAddr,       // operands: [object_ptr, prop_offset]; for Phase B2
```

`ArrayElementAddr` is `Heap(Array) + Int → I64`. It emits:
- ARM64: load array ptr, load index, compute `ptr + 16 + index*8` (packed)
  or call `__rt_hash_element_addr(ptr, key)` for assoc.
- x86_64: same arithmetic with the x86_64 register conventions.

The lowering (`src/ir_lower/expr/mod.rs`) gets a new by-ref arg path:

```rust
fn lower_by_ref_element_arg(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    span: Span,
) -> ValueId {
    // For a packed array with statically-known element type, emit
    // Op::ArrayElementAddr(array_ptr, index). For a Mixed/assoc array,
    // emit the hash-element-address runtime helper. Append targets
    // ($a[]) lower to "address of the next slot after auto-grow".
}
```

`lower_arg_with_signature` (line 4240) gains, before the `lower_expr(arg)`
fallthrough:

```rust
if sig.ref_params.get(index).copied().unwrap_or(false) {
    if let ExprKind::ArrayAccess { array, index: idx } = &arg.kind {
        return lower_by_ref_element_arg(ctx, array, idx, arg.span);
    }
    if let ExprKind::PropertyAccess { .. } = &arg.kind {
        return lower_by_ref_property_arg(ctx, arg);  // Phase B2
    }
}
```

**Codegen**: `src/codegen_ir/lower_inst.rs` — add `Op::ArrayElementAddr =>`
to the dispatch (line ~155) routing to a new
`src/codegen_ir/lower_inst/arrays.rs::lower_array_element_addr` (or reuse
`arrays.rs` if it has a natural home). This emits the address arithmetic
per target, with column-81 comments on every instruction.

**Append target `$a[]`**: this is a by-ref write target in PHP. Lower it as
"ensure capacity for one more, return address of the new slot." Reuse the
existing array-grow runtime helpers (`__rt_array_push_*`). The new element
is initialized to `null`/`0` before the callee runs.

**Writeback**: for the direct-address path, **no writeback** is needed — the
callee writes through the pointer into the array storage. The existing
`plan_ref_arg_writebacks` Mixed-cell writeback path is bypassed for these
args (the source value is an `ArrayElementAddr` result, not a `LoadLocal`,
so `local_ref_arg_source` fails and `materialize_temporary_ref_arg_cell` is
skipped via a new guard that recognizes the element-address opcode).

**Phase B2 — Object properties** (`$o->prop` by-ref): out of scope for this
fix unless trivial; the issue only mentions array elements. The
`is_by_ref_lvalue` predicate accepts properties so the checker doesn't
reject them, but the lowering emits a clear `unsupported` diagnostic for
property by-ref until a follow-up. Document this in `docs/php/`.

**Tests**:
- `tests/error_tests/callables.rs`: update the existing tests at lines 232,
  244, 257 that assert "must be passed a variable" — those pass non-lvalue
  args (literals/calls) and must *still* error. Add new tests asserting
  `$a[0]`, `$a['k']`, `$a[]`, `$a[$i]` are *accepted*.
- `tests/codegen/callables/by_ref_array_element.rs` (new): the issue program
  plus nested arrays, append targets, and Mixed-element arrays.

### Test plan
| # | PHP source | Expected | Covers |
|---|---|---|---|
| 1 | `function bump(&$x){$x++;} $a=[5]; bump($a[0]); echo $a[0];` | `6` | Issue #360 exact |
| 2 | `function bump(&$x){$x++;} $a=['k'=>5]; bump($a['k']); echo $a['k'];` | `6` | String key |
| 3 | `function bump(&$x){$x++;} $a=[1,2,3]; bump($a[1]); echo implode(',',$a);` | `1,3,3` | Mid-array element |
| 4 | `function bump(&$x){$x+=10;} $a=[[1],[2]]; bump($a[0][0]); echo $a[0][0];` | `11` | Nested array element |
| 5 | `function bump(&$x){$x++;} $a=[]; bump($a[]); echo $a[0];` | `1` | Append target (`$a[]`) |
| 6 | `function s(&$x){$x='changed';} $a=['k'=>'orig']; s($a['k']); echo $a['k'];` | `changed` | String value by-ref |
| 7 | `function add(&$arr,$v){$arr[]=$v;} $a=[1]; add($a,2); echo implode(',',$a);` | `1,2` | Array by-ref + append (existing path, lock in) |
| 8 | `function b(&$x){$x++;} b(5);` | error "must be passed a variable" | Non-lvalue still rejected |
| 9 | `function b(&$x){$x++;} b(foo());` | error | Call result rejected |
| 10 | `function b(&$x){$x++;} $a=[1]; b($a[0]+1);` | error | Binary op rejected |

### Risk assessment
- **Highest risk** of the five issues: introduces a new EIR op and a new
  codegen path for element-address arithmetic on two targets. Element byte
  sizes (8 for scalars, 16 for TaggedScalar, cell-pointer indirection for
  Mixed) must be exactly right per `PhpType::stack_size` and the array
  header layout in `src/codegen/runtime/arrays/`.
- **Array grow during by-ref**: if the callee appends to the same array it
  received an element address for, the array may reallocate and invalidate
  the passed pointer. PHP does not guarantee stability here either (a by-ref
  array param that grows can move the element), so matching PHP's "best
  effort" is acceptable. Document.
- **Mixed-element address**: for `Array(Mixed)`, the element is a boxed
  cell; passing the cell pointer is correct. For `AssocArray`, the hash
  bucket's value cell address is needed; a runtime helper
  `__rt_hash_element_addr(hash, key)` is the clean approach (mirror
  `__rt_hash_iter_next`'s lookup).
- **Regression watch**: every existing by-ref test in
  `tests/codegen/callables/**`, `tests/ir_backend_parity/cases.rs` (the
  `parity_*_by_ref_*` tests at lines 1518, 1540, 1560), and
  `tests/error_tests/callables.rs`. The frozen legacy backend must *not* be
  touched; its by-ref path stays as-is (it may already support array
  elements — verify parity but do not change it).
- **Runtime cache**: if a new runtime helper (`__rt_hash_element_addr`) is
  added, `~/.cache/elephc/*.o` must be cleared before the next compile
  (`rm -rf ~/.cache/elephc`). Note this in the PR.

---

## Cross-cutting notes

- **Order of implementation**: #340 and #377 are small, low-risk, and can
  land first (regression locks + residual fixes). #384 is medium (optimizer
  propagation change). #381 is small-medium (iterator state slot). #360 is
  large (new EIR op + codegen path); land it last and consider splitting
  into "Phase A checker accept" and "Phase B lowering" PRs so PHP programs
  that only need the checker to stop erroring can progress sooner (Phase A
  alone makes `bump($a[0])` *compile* but the mutation is still lost —
  document this clearly).
- **Test policy**: each issue's tests span the required four surfaces
  (lexer/parser/codegen/error) where applicable, plus an `examples/` entry
  per AGENTS.md. Suggested examples:
  - `examples/by_ref/main.php` exercising #384 and #360,
  - `examples/foreach_snapshot/main.php` for #381,
  - `examples/interpolation/main.php` for #340,
  - `examples/type_widening_loop/main.php` for #377.
- **Docs**: update `docs/php/functions.md` (by-ref parameters), the
  interpolation section of `docs/php/strings.md`, the foreach section of
  `docs/php/control-structures.md`, and the optimizer section of
  `docs/internals/the-optimizer.md` to describe intra-expression
  write-invalidation.
- **CI**: rely on the sharded codegen matrix for the full ARM64/x86_64
  coverage; locally run only the focused filters named in each issue's test
  plan during implementation.
