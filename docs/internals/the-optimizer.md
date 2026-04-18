---
title: "The Optimizer"
description: "How elephc folds constants and prunes dead code before code generation."
sidebar:
  order: 6
---

**Source:** `src/optimize.rs`

elephc's optimizer is intentionally simple and AST-focused. It does not build a separate IR or run heavyweight SSA passes. Instead, it performs a small set of local rewrites that already pay off in generated assembly quality and compile-time clarity.

Today the optimizer is split into two passes:

1. `fold_constants(program)` runs before type checking
2. `prune_constant_control_flow(program)` runs after successful type checking and warning collection

That split matters. Some rewrites are always safe on syntax alone, while others should only happen after diagnostics have already seen the checked program.

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

## Pass 2: Post-check pruning

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

### Example

```php
<?php
if (true) {
    echo "kept\n";
} else {
    echo pow(3, 4) . "\n";
}
```

After pruning, the dead branch disappears entirely. That means codegen never emits the `pow` path.

## Why there are two passes

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
- prune larger dead control-flow only after checking

## Conservatism and side effects

The optimizer is intentionally conservative about what counts as "pure".

It does **not** assume purity for operations such as:

- function calls
- method calls
- object creation
- property access
- array access
- buffer allocation
- increment/decrement
- `throw`

That conservatism is why the pass is safe to run by default: if an expression could have runtime behavior, the optimizer prefers to keep it.

## What the optimizer does not do yet

The current pass is local. It does not yet implement:

- constant propagation across variables and statement boundaries
- interprocedural optimization
- backend-specific peephole cleanup
- runtime dead stripping
- register allocation

Those remain roadmap items for later optimization work.
