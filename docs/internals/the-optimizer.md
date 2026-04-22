---
title: "The Optimizer"
description: "How elephc folds constants, prunes control flow, and eliminates dead code before code generation."
sidebar:
  order: 6
---

**Source:** `src/optimize.rs`

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
- `??` and ternary when the selected result is already known
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
- conservative `switch` merges when all surviving exit paths agree on the same scalar value
- conservative `try` / `catch` merges when every fallthrough handler path agrees on the same scalar value
- recognizing uniform scalar assignment outcomes from local merge expressions such as `?:` and `match`
- recognizing scalar locals introduced by destructuring fixed scalar array literals with `list(...)` / `[...] = [...]`
- preserving untouched scalar locals across simple loops when a conservative local write analysis can prove the loop only mutates other variables, including simple nested `switch`, `try/catch/finally`, `foreach`, other simple nested loop statements, local array writes like `$items[] = $i` / `$items[0] = $i`, local property writes like `$box->last = $i` / `$box->items[] = $i`, and targeted invalidations like `unset($tmp)`, while also retaining stable scalar values introduced by `for` init clauses
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
- strict scalar guards now feed the same pruning: after checks like `$x === null`, `$x === 0`, or `$x === ""`, nested regions that contradict the exact known value can be removed
- negative branches of strict scalar checks now contribute exclusion facts too, so `else` paths after checks like `$x === 0` or the true path of `$x !== null` can prune nested contradictions without needing a full exact replacement value
- the same strict scalar guard machinery now covers exact floats as well as PHP-falsy strings like `""` and `"0"`, so nested truthiness checks and strict literal contradictions can be pruned when those values are already known or excluded
- outer exact scalar guards can now also prune impossible `switch` entries: when a `switch ($x)` subject or a `switch (true|false)` guard pattern is already decided by surrounding strict checks, dead leading cases are dropped before the remaining switch body is analyzed, and the CFG-lite pass can also drop later `switch` blocks that no longer have any reachable predecessor after an exact entry is chosen
- cumulative false guards in `if` / `elseif` chains can now prune later impossible branches and unreachable `else` suffixes before codegen, instead of carrying logically dead tails through the rest of the pipeline
- `switch (true|false)` now applies the same cumulative guard idea across case fallthrough: later guard-like cases and the `default` can be pruned when earlier no-match paths already force an exhaustive outcome
- multi-pattern `switch (true|false)` cases now participate in that same cumulative reasoning, so an exhaustive label set inside one case can remove later dead cases and the `default`
- exact scalar guards now drive the same pruning inside ordinary `switch ($x)` multi-pattern cases: impossible labels inside one case are dropped, and if a surviving later label is guaranteed to match, later dead cases and `default` are removed as well
- excluded scalar guards now also prune ordinary `switch ($x)` entries, so outer facts like `$x !== 1` can remove dead `case 1:` labels even when the exact runtime value of `$x` is still unknown
- truthiness facts now also feed ordinary `switch ($x)` pruning for `case true` / `case false`: cumulative no-match paths can eliminate dead boolean cases and even remove a dead `default` once the remaining truthiness paths are fully covered
- that same truthiness pruning now preserves earlier `Unknown` multi-pattern entries as reachable CFG entry points, so we do not over-prune preceding case bodies while still removing dead boolean suffixes and `default`
- `switch (true|false)` cases using single guard-like patterns can feed the same internal region pruning inside the selected case body, again with local-write invalidation to stay conservative
- `catch` and `finally` bodies now invalidate outer guard facts only for locals written on the relevant pre-handler paths, so nested pruning there stays sound without discarding unrelated guard facts
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

## What the optimizer does not do yet

The current optimizer is still intentionally local. It does not yet implement:

- CFG-aware or fixed-point constant propagation across wider loops and general path merges
- richer memory-model-aware propagation across heap-backed locals and broader aliasing situations
- deeper exception-aware dead-code elimination beyond conservative `try` heuristics
- broader control-flow normalization beyond the current local AST shell rewrites
- backend-specific peephole cleanup
- runtime dead stripping
- register allocation

Those remain roadmap items for later optimization work.
