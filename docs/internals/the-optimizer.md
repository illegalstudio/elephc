---
title: "The Optimizer"
description: "How elephc folds constants, prunes control flow, and eliminates dead code before code generation."
sidebar:
  order: 6
---

**Source:** `src/optimize/`

elephc's optimizer is intentionally simple and AST-focused. It does not build a separate IR or run heavyweight SSA passes. Instead, it performs a small set of local rewrites that already pay off in generated assembly quality and compile-time clarity.

Today the optimizer is split into five passes:

1. `fold_constants(program)` runs before type checking
2. `propagate_constants(program)` runs after successful type checking
3. `prune_constant_control_flow(program)` runs after propagation and warning collection
4. `normalize_control_flow(program)` runs after pruning and rewrites structurally equivalent control-flow shells into simpler AST shapes
5. `eliminate_dead_code(program)` runs after normalization and removes leftover unreachable or non-observable statements from the already-normalized AST

That split matters. Some rewrites are always safe on syntax alone, while others should only happen after diagnostics have already seen the checked program.

Alongside those five passes, the optimizer also builds lightweight local **effect summaries**. These summaries answer two questions conservatively:

- does this expression have observable side effects?
- can this expression throw?

That effect information is what lets later pruning and dead-code elimination stay more precise around `try` / `catch`, callable aliases, and non-trivial control-flow merges.

## Why optimize at the AST level

elephc goes straight from AST to target assembly. There is no middle IR for optimization to target, so the cheapest high-value place to simplify code is the AST itself.

This gives us a few immediate wins:

- less work for codegen
- smaller and clearer generated assembly
- fewer runtime helper calls for expressions whose result is already known
- a conservative place to prune dead branches without committing to backend-specific machinery

Examples:

```php
<?php
echo 2 ** 3;
echo "hello " . "world";
echo (int)"42";
```

By the time codegen sees this, it can already emit constants instead of calling runtime helpers such as `pow`, `__rt_concat`, or numeric string conversion paths.

## Pass 1: Constant folding

`fold_constants()` walks the AST recursively and rewrites expressions whose result is statically decidable from their children alone.

Current folding coverage includes:

- scalar arithmetic: `+`, `-`, `*`, `/`, `%`, `**`
- bitwise and shift ops on integers
- unary `-`, `!`, and `~`
- string-literal concatenation with `.`
- strict comparisons and numeric comparisons
- logical `&&` / `||` when both sides are scalar constants
- spaceship `<=>`
- `??`, ternary, and `match` when the selected result is already known
- scalar indexed and associative array-literal reads such as `[2, 9][0]` and `["a" => 2]["a"]` when every literal entry is scalar
- scalar casts such as `(int)"42"` or `(bool)"0"` when the semantics are unambiguous
- recursive folding inside:
  - function and method bodies
  - closures and arrow functions
  - default parameter values
  - property defaults
  - constant declarations

### Example

```php
<?php
$x = (2 < 3) ? (2 ** 3) : (3 ** 4);
echo $x . "\n";
```

After folding, the AST is effectively:

```php
<?php
$x = 8;
echo $x . "\n";
```

## Pass 2: Local constant propagation

`propagate_constants()` runs after type checking, when the checker has already validated the original program structure.

This pass is still intentionally local and conservative. Today it focuses on:

- straight-line scalar local assignments such as:
  - `$x = 2;`
  - `$y = 3;`
  - `echo $x ** $y;`
- simple `if` merges where every fallthrough branch agrees on the same scalar value
- conservative `switch` merges when all possible exit paths agree on the same scalar value
- known-subject `switch` merges that only simulate the selected entry and its fallthrough suffix
- conservative `try` / `catch` merges when every reachable fallthrough handler path agrees on the same scalar value
- non-throwing `try` bodies that keep unreachable `catch` writes out of the post-`try` constant environment
- recognizing uniform scalar assignment outcomes from local merge expressions such as `?:` and `match`
- recognizing scalar locals introduced by destructuring fixed scalar array literals with `list(...)` / `[...] = [...]`
- preserving untouched scalar locals across simple loops when a conservative local write analysis can prove the loop only mutates other variables, including simple nested `switch`, `try/catch/finally`, `foreach`, other simple nested loop statements, local array writes like `$items[] = $i` / `$items[0] = $i`, local property writes like `$box->last = $i` / `$box->items[] = $i`, and targeted invalidations like `unset($tmp)`, while also retaining stable scalar values introduced by `for` init clauses
- local loop path summaries for known `while(false)`, `do...while(false)`, `while(true)` / `for(;;)` break exits, and branch-local loop exits that agree on scalar values
- re-running constant folding on expressions after substitutions are made
- propagating into nested bodies conservatively without trying to solve full data-flow across loops or general path-sensitive CFGs

### Example

```php
<?php
if ($argc > 0) {
    $base = 2;
} else {
    $base = 2;
}

echo $base ** 3;
```

After propagation, the later `echo` effectively becomes:

```php
<?php
echo 8.0;
```

That means later passes never need to emit the runtime `pow` path.

## Pass 3: Post-check control-flow pruning

`prune_constant_control_flow()` runs only after type checking succeeds. This pass is allowed to remove dead branches and dead statements because diagnostics have already seen the checked structure.

Current pruning coverage includes:

- `if` / `elseif` / `else` chains with constant conditions
- `while (false)`
- `do { ... } while (false)` reduced to a single execution of the body
- `for (...; false; ...)`, preserving the `init` clause but removing dead loop/update work
- `match` expressions whose subject and patterns are statically decidable
- shadowed `match` arms and duplicate arm patterns removed when earlier arms already own the same exact pattern entries
- `switch` pruning when early case prefixes are provably impossible
- unreachable statements after:
  - `return`
  - `throw`
  - `break`
  - `continue`
- dead code after exhaustive `if` / `else`
- dead code after conservative exhaustive `switch ... default`
- pure expression statements whose result is unused
- pure dead subexpressions inside:
  - ternaries
  - `??`
  - short-circuit `&&` / `||`

## Pass 4: Control-flow normalization

`normalize_control_flow()` runs after the pruning pass. At this point the AST already has constant-dead branches removed, so the job becomes "reshape the remaining control flow into simpler but equivalent forms" rather than "decide which branch is dead".

Current normalization coverage includes:

- empty `ifdef`, `if`, `switch`, and degenerate `try` shells
- single-path conditionals such as:
  - `if ($cond) {} else { ... }` → `if (!$cond) { ... }`
  - nested single-path `if` chains collapsed into one condition with `&&`
- `if` statements whose `then` and `else` bodies normalize to the same block collapsed into “evaluate the condition only if observable, then run the shared block once”
- `elseif` chains canonicalized into nested `else { if (...) { ... } }` form
- adjacent `if` chain heads with identical bodies merged into one `if ($a || $b) { ... }` shape
- adjacent `if` chain tails with identical fallback merged into one `if (!$a && $b) { ... } else { ... }` shape
- longer `if` chains repeatedly normalized until these shapes saturate
- adjacent `switch` cases with identical bodies merged into a single multi-pattern case
- pure fallthrough `switch` labels folded into the next non-empty case body
- single live `switch` cases rewritten to `if` when the loose comparison can be reconstructed safely
- adjacent `catch` clauses with the same body and variable merged into a single deduplicated, stably ordered multi-type catch
- constant `switch` execution materialized into the exact statement tail that would run, preserving fallthrough and `break`
- non-throwing `try` / `catch` simplification
- outer `finally` blocks folded into a single inner `try` when they wrap exactly one inner `try` that does not already have its own `finally`
- safe hoisting of non-throwing, fallthrough prefixes out of `try` blocks
- conservative flattening of `try` / `finally` when the `try` body cannot throw and the body falls through
### Example

```php
<?php
try {
    echo "a";
    throw new Exception("boom");
} catch (Exception $e) {
    echo "b";
}
```

The leading `echo "a";` is known not to throw, so the optimizer can hoist it out of the `try` and leave only the actually-throwing tail protected by the handler.

## Pass 5: Dead-code elimination

`eliminate_dead_code()` now runs after normalization. At this point the AST has already had constant-dead branches removed and redundant control-flow shells compacted, so the job becomes "drop the leftovers" rather than "reshape the program".

Current dead-code-elimination coverage includes:

- unreachable statements after:
  - `return`
  - `throw`
  - `break`
  - `continue`
- statements after exhaustive `try/catch` and `try/finally` exits
- unreachable `catch` paths when the post-DCE `try` body can no longer throw
- shadowed `catch` clauses whose exception types are already fully covered by earlier handlers, including all later handlers after `catch (Throwable ...)`
- shadowed `switch` patterns whose match points are already covered by earlier case labels, including full-case removal or fallthrough-body merging when no entry pattern remains
- internal `if` regions pruned when outer pure variable guards or strict boolean checks already determine a nested branch outcome, with guard invalidation on relevant local writes to stay conservative
- guard-based pruning now also understands simple pure `&&` / `||` combinations, so contradictions like `if ($a && $b) { if (!$a || !$b) ... }` can be removed without needing constant folding first
- loose equality and safe relational-comparison complements now feed the same guard model, so nested checks like `$x == 0` followed by `$x != 0`, or `$x > 10` followed by `$x <= 10`, can be pruned when the outer branch proves the contradiction
- strict scalar guards now feed the same pruning: after checks like `$x === null`, `$x === 0`, or `$x === ""`, nested regions that contradict the exact known value can be removed
- negative branches of strict scalar checks now contribute exclusion facts too, so `else` paths after checks like `$x === 0` or the true path of `$x !== null` can prune nested contradictions without needing a full exact replacement value
- the same strict scalar guard machinery now covers exact floats as well as PHP-falsy strings like `""` and `"0"`, so nested truthiness checks and strict literal contradictions can be pruned when those values are already known or excluded
- outer exact scalar guards can now also prune impossible `switch` entries: when a `switch ($x)` subject or a `switch (true|false)` guard pattern is already decided by surrounding strict checks, dead leading cases are dropped before the remaining switch body is analyzed, and the CFG-lite pass can also drop later `switch` blocks that no longer have any reachable predecessor after an exact entry is chosen
- cumulative false guards in `if` / `elseif` chains can now prune later impossible branches and unreachable `else` suffixes before codegen, instead of carrying logically dead tails through the rest of the pipeline
- `switch (true|false)` now applies the same cumulative guard idea across case fallthrough: later guard-like cases and the `default` can be pruned when earlier no-match paths already force an exhaustive outcome
- direct-entry `switch (true|false)` case bodies can also inherit cumulative no-match guards from earlier non-fallthrough cases, so nested contradictions inside later case bodies are pruned while fallthrough remains conservative
- multi-pattern `switch (true|false)` cases now participate in that same cumulative reasoning, so an exhaustive label set inside one case can remove later dead cases and the `default`
- exact scalar guards now drive the same pruning inside ordinary `switch ($x)` multi-pattern cases: impossible labels inside one case are dropped, and if a surviving later label is guaranteed to match, later dead cases and `default` are removed as well
- excluded scalar guards now also prune ordinary `switch ($x)` entries, so outer facts like `$x !== 1` can remove dead `case 1:` labels even when the exact runtime value of `$x` is still unknown
- truthiness facts now also feed ordinary `switch ($x)` pruning for `case true` / `case false`: cumulative no-match paths can eliminate dead boolean cases and even remove a dead `default` once the remaining truthiness paths are fully covered
- that same truthiness pruning now preserves earlier `Unknown` multi-pattern entries as reachable CFG entry points, so we do not over-prune preceding case bodies while still removing dead boolean suffixes and `default`
- truthiness facts also prune scalar literal labels of the opposite truthiness in ordinary `switch ($x)`, so truthy/falsy outer guards can remove dead `case 0`, `case ""`, `case null`, or analogous truthy literal labels inside mixed multi-pattern switches
- `switch (true|false)` cases using single guard-like patterns can feed the same internal region pruning inside the selected case body, again with local-write invalidation to stay conservative
- `catch` and `finally` bodies now invalidate outer guard facts only for locals written on the relevant pre-handler paths, so nested pruning there stays sound without discarding unrelated guard facts
- throw-path invalidation for `switch` now consults the CFG-lite reachable block set, so writes in impossible case bodies do not unnecessarily kill catch-body guards, while reachable case writes before a `throw` still invalidate them
- catch-side guard invalidation is now path-aware: writes that only happen on non-throwing `try` paths no longer block pruning inside the `catch`
- condition-only empty `if` / `elseif` chains reduced to just the observable condition checks that still matter
- empty `elseif` bodies in the middle of a live chain folded into the minimum negated guard needed for later branches
- trailing block tails sunk into `if` and `ifdef` fallthrough branches, so later statements are only retained on paths that can still reach them
- trailing block tails sunk into `switch` suffixes when later code is reached deterministically either by falling off the final reachable path or by exiting a case via `break`
- trailing block tails sunk into `try` / `catch` fallthrough paths, and into `finally` only in the conservative case where every pre-finally path must still fall through
- trailing empty `switch` labels dropped when they no longer lead to reachable work
- pure expression statements whose result is unused
- pure expression statements that become exposed by earlier normalization

The current path-aware DCE work uses small path-outcome helpers for `if`, `ifdef`, `switch`, and `try`, all speaking the same local tail-path vocabulary (`falls through`, `breaks`, `no tail`, `unknown`). That lets tail-sinking and shell collapsing share one reachability model instead of duplicating ad-hoc logic per statement shape.

The first `dead-code-elimination v3` slices also start moving some of that reasoning onto a tiny CFG-lite layer. Today that covers `switch`, `if`, and `try/catch/finally`: branch bodies are lowered to small basic-block graphs and their tail reachability is classified from successor edges instead of only from hand-written scans. It is still AST-local, not a full function CFG, but it is the first step toward block-aware DCE.

### Example

```php
<?php
if (true) {
    echo "kept\n";
} else {
    echo pow(3, 4) . "\n";
}
```

After pruning and normalization, the dead branch disappears entirely. The final dead-code pass then has less structural noise to inspect, and codegen never emits the `pow` path.

## Effect summaries: purity and `may_throw`

The optimizer now maintains a small local effect-analysis layer that sits underneath the pruning and dead-code-elimination passes.

Current coverage includes:

- known pure / non-throwing builtins such as `strlen()`
- user-defined functions whose bodies are themselves pure / non-throwing
- user-defined static methods with the same conservative summary inference
- private instance methods called on `$this`, where dispatch is statically known
- direct closure calls and local closure aliases
- named first-class callables and expr-calls on those callables
- callable aliases that survive merges through:
  - `if` / `else`
  - `try` / `catch` / `finally`
  - `switch`
- callable-producing expressions such as:
  - ternaries
  - `??`
  - `match`
  when every surviving branch agrees on the same callable effect

This analysis is still intentionally local. It does not try to solve general whole-program purity. Instead, it focuses on the small set of call shapes that matter most for AST rewriting today.

### Example

```php
<?php
$f = match ($mode) {
    1 => strlen(...),
    default => strlen(...),
};

try {
    echo $f("abc");
} catch (Exception $e) {
    echo pow(2, 8);
}
```

Because every `match` arm produces the same known pure / non-throwing callable, the optimizer can prove that the `catch` path is dead and avoid emitting the `pow` branch at all.

## Why there are five passes

If elephc removed whole branches before type checking, it could accidentally hide useful diagnostics.

For example, imagine:

```php
<?php
if (false) {
    $x = "hello";
    $x = 123;
}
```

From an optimization point of view that block is dead. From a compiler UX point of view, it may still be valuable for the checker and warning passes to see it before any aggressive pruning happens.

So the current rule is:

- fold obvious pure scalar expressions early
- propagate known scalar locals only after checking
- prune larger dead control-flow only after checking
- normalize the remaining control-flow into simpler equivalent shapes
- run structural dead-code cleanup only after those earlier passes have already simplified the tree

## Conservatism and side effects

The optimizer is intentionally conservative about what counts as "pure" or "non-throwing".

It now recognizes a useful subset of call expressions precisely, but it still does **not** assume purity for broad dynamic operations such as:

- unknown function or method calls
- dynamic instance dispatch beyond the statically-known `$this` / private-method case
- object creation
- most property and array reads where runtime hooks or dynamic behavior could matter
- buffer allocation
- increment/decrement
- `throw`

That conservatism is why the pass is safe to run by default: if an expression could have runtime behavior and elephc cannot prove otherwise with its local summaries, the optimizer prefers to keep it.

## Pipe operator optimizations

PHP 8.5's pipe operator `|>` is implemented as a dedicated `ExprKind::Pipe`
node rather than a `BinOp`, which lets the optimizer reason about it as a
first-class call site rather than an opaque binary operation. Three layers of
optimization apply, end to end:

### Effect modelling

`ExprKind::Pipe { value, callable }` in `src/optimize/effects.rs` combines
the effects of `value`, the effects of `callable`, and the effects of
*invoking* `callable` via `expr_call_effect`. Because the per-target call
effect already collapses to `Effect::PURE` when the callable is a
first-class callable referencing a pure built-in (`strlen`, `strtoupper`, …),
a pure pipe expression statement is observably dead and the DCE pass
removes it. No extra wrapping with `with_side_effects()` is applied — that
would mask the precise effect of the target and defeat DCE.

### Constant folding for pure pipes

`src/optimize/fold/pipes.rs::try_fold_pure_pipe` is called from
`fold_expr`'s Pipe branch. When the value is a literal scalar (or
literal array, for predicates) and the callable is `FirstClassCallable(
Function(name))` referencing a whitelisted pure built-in, the pipe folds to
a literal at compile time. Examples:

- `"hello" |> strlen(...)` → `5`
- `3.7 |> floor(...)` → `3.0`
- `"hello" |> strtoupper(...) |> strrev(...)` → `"OLLEH"` (each stage
  folds and feeds the next)
- `5 |> is_int(...)` → `true`
- `5 |> gettype(...)` → `"integer"`

The whitelist is intentionally narrow: only built-ins whose Rust
implementation is byte-equivalent to PHP for the literal types we accept.
Non-ASCII strings, `NaN`/infinity floats, and `i64::MIN` for `abs` fall
back to the runtime call so PHP semantics are preserved.

### First-class-callable short-circuit (codegen-side)

Tracked from the optimizer's perspective because the type checker's
`first_class_callable_targets` map is mirrored into the codegen `Context`
and used to bypass the closure wrapper at call sites. `$cb = foo(...)`
followed by `$x |> $cb` lowers to a direct `bl _fn_foo` instead of an
indirect call through the FCC wrapper. The same short-circuit fires for
`$cb(args)` outside the pipe operator — `array_map`, `call_user_func`, or
plain `$cb()` calls all benefit.

`self::method(...)` and `parent::method(...)` are pre-resolved to
`Named(class)` at FCC variable storage time, so the short-circuit applies
to them too. `static::method(...)` keeps its `Static` receiver in storage
and the call site re-uses the caller-scope's `__elephc_fcc_called_class_id`
/ `__elephc_called_class_id` / `__elephc_fcc_this` / `$this` chain — the
exact same chain the closure wrapper would consult, just without the
wrapper trampoline.

### Dead-wrapper stubbing

When a first-class callable is assigned to a local and every read of that
local is short-circuited (the variable never reaches `emit_variable`,
`emit_closure_call`'s fallback path, `call_user_func`, `array_map`,
`Fiber::new`, etc.), the deferred wrapper body is replaced by a tiny
`mov #0; ret` stub. The address load at the assignment site stays
resolvable, the slot still receives a value, and the binary loses the
~50–100 instructions of dead prologue/body/epilogue per uninvoked
wrapper. `Context::mark_fcc_used` is the central hook every "this FCC
value escapes" path calls.

## What the optimizer does not do yet

The current optimizer is still intentionally local. It does not yet implement:

- full fixed-point/basic-block constant propagation across arbitrary loops and general path merges
- richer memory-model-aware propagation across heap-backed locals and broader aliasing situations
- exact exception-type reachability, nested rethrow modeling, and less conservative `finally` invalidation beyond the current path-aware `try` heuristics
- broader guard reasoning for range facts and multi-variable relationships beyond the current boolean, scalar, loose-comparison, and safe relational-complement facts
- broader control-flow normalization beyond the current local AST shell rewrites
- backend-specific peephole cleanup
- runtime dead stripping
- elimination of the `adrp/add/stur` instruction triple at the FCC assignment site when the wrapper is stubbed (the stub address still gets loaded and stored even though both are dead)

Those remain roadmap items for later optimization work.

Note that **register allocation** is no longer on this list: the EIR backend now
runs a linear-scan register allocator (`src/ir_passes/`), described in
[The IR](the-ir.md). It is a backend pass over EIR rather than an AST-level
optimization, so it lives outside `src/optimize/`.

IR-level transformations also no longer live here. The EIR backend runs a
fixed-point pass driver (`src/ir_passes/driver.rs`) after lowering, starting with
identity arithmetic folding (`x + 0`, `x * 1`, `x ^ x`, …) and local peephole
patterns (box/unbox cancellation, scalar load/store forwarding, paired
acquire/release cancellation, string-literal concat folding, redundant
`move`/`borrow` cleanup). These are rewrites the AST optimizer cannot express
well because they need value identity, basic blocks, or dominance; see
[Optimization Passes](the-ir.md#optimization-passes).
