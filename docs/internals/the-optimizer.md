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
- pure expression statements whose result is unused
- pure expression statements that become exposed by earlier normalization

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
