---
title: "Optimization and codegen controls"
description: "Controls that affect generated code quality and shape: the EIR optimization passes (--ir-opt), the register allocator (--regalloc), and the null representation (--null-repr)."
sidebar:
  order: 5
---

elephc optimizes in two places: over the AST before lowering, and over EIR after
lowering. The AST optimizer is always on. The EIR-level controls below let you
turn passes off for benchmarking and diagnostics, and choose between code-shape
trade-offs.

## Two optimization layers

- **AST optimizer** — PHP-preserving rewrites expressed over syntax: constant
  folding, constant propagation, control-flow pruning and normalization, and
  dead-code elimination. Always on; not behind a flag. See
  [The Optimizer](../internals/the-optimizer.md).
- **EIR optimization passes** — transformations that need value identity, basic
  blocks, or dominance, which the AST cannot express well. Run by a fixed-point
  pass driver after lowering. Controlled by `--ir-opt`. See
  [The EIR Design](../internals/the-ir.md#optimization-passes).

## EIR optimization passes

After the AST is lowered to EIR and validated, a fixed-point pass driver runs the
registered transformation passes over each function until none reports a change.
The passes are **on by default**.

```bash
# Default: EIR optimization passes enabled
elephc hot.php

# Disable them (for A/B comparison or diagnostics)
elephc --no-ir-opt hot.php
elephc --ir-opt=off hot.php

# Explicit enable
elephc --ir-opt=on hot.php
```

The environment variable `ELEPHC_IR_OPT=off` disables the passes for a whole run
without editing each command.

In debug and test builds the driver re-validates each function after **every**
pass and aborts if a pass produced malformed IR, so optimization bugs surface
immediately during development. In release builds those checks are compiled out.

### Identity arithmetic folding

The first registered pass folds algebraic identities on integer and float
arithmetic and bitwise operations:

| Pattern | Result |
|---|---|
| `x + 0`, `0 + x`, `x - 0` | `x` |
| `x * 1`, `1 * x`, `x / 1` | `x` |
| `x \| 0`, `x ^ 0`, `x << 0`, `x >> 0` | `x` |
| `x & x`, `x \| x` | `x` |
| `x * 1.0`, `x / 1.0` | `x` |
| `x ^ x`, `x - x`, `x * 0`, `x & 0`, `x % 1` | `0` |

Only PHP-equivalent rewrites are applied: integer `x / 0` and `x % 0` are left to
trap at runtime, and float additive-zero (`x + 0.0`) and `x * 0.0` are excluded
because signed zero and `NaN` make them observable.

You can see the effect directly with [`--emit-ir`](output-and-diagnostics.md#--emit-ir):

```bash
# With passes on, `$argc * 1` folds away; with --no-ir-opt it stays an `imul`.
elephc --emit-ir app.php
elephc --emit-ir --no-ir-opt app.php
```

This is a peephole-level optimization. It speeds up code that contains redundant
identity operations in hot paths and is a no-op on code that does not — unlike
register allocation, which helps broadly.

### Peephole patterns

The second registered pass applies local rewrites that clean up the shape of
lowered EIR. Each is refcount-balanced and produces output identical to PHP:

| Pattern | Rewrite |
|---|---|
| Box/unbox cancellation | `unbox(box(x))` → `x` for scalar (`NonHeap`) payloads |
| Redundant `move`/`borrow` | a forwarding op whose result has the same ownership and type as its operand folds to the operand |
| Load/store forwarding | a `load` of a scalar local right after a `store` to it reads the stored value directly |
| Dead store | storing a scalar local the value it already holds is removed |
| Acquire/release cancellation | an `acquire` whose result is consumed only by its `release` drops both |
| String-literal concat folding | `concat("a", "b")` → the single literal `"ab"` |

Load/store forwarding and dead-store removal apply only to non-aliased scalar
locals, so reference semantics and copy-on-write are never affected. The
remaining patterns only fold when ownership and type are preserved, so cleanup
and refcounting stay balanced.

You can see the effect with [`--emit-ir`](output-and-diagnostics.md#--emit-ir):
`$x = $argc; echo $x;` forwards the load so the `echo` reads the stored value and
the `load_local` becomes a `nop`.

### Dead instruction elimination

The third registered pass computes CFG liveness and neutralizes unused
result-producing instructions whose effect metadata says they are pure. This
cleans up dead values exposed by earlier EIR rewrites. For example, identity
folding can turn `$argc + 0` into `$argc`; dead-instruction elimination then
drops the now-unused `const_i64 0` from optimized EIR.

The pass is deliberately conservative. It keeps read-only, allocation,
mutation, refcounting, output, warning, fatal, throw, and deopt-capable
instructions even when their results are unused. The goal is to remove dead pure
arithmetic and literal scaffolding without changing PHP-visible behavior or
ownership cleanup.

You can compare the shape directly:

```bash
elephc --emit-ir app.php
elephc --emit-ir --no-ir-opt app.php
```

### Dead store elimination

The fourth registered pass removes `store_local` writes whose value is never read
before the slot is overwritten or the function exits. It computes backward,
CFG-aware liveness over local slots (a `load_local` makes a slot live, a
`store_local` kills it) so a dead store is dropped even when the overwrite is in a
later block. Unlike the peephole's per-block, value-equality store drop, this pass
is liveness-based and crosses block boundaries, so it removes a store of a
*different* value whose result is never observed.

It is restricted to non-refcounted scalar locals that are accessed only through
plain `load_local`/`store_local` and never escape by reference. Refcounted slots
are wrapped by separate `acquire`/`release` ownership ops, and by-reference call
arguments or closure captures alias the slot through its `load_local`, so both are
left untouched to keep reference counting and aliasing semantics intact.

### Branch simplification

The fifth registered pass prunes the control-flow graph three ways:

- **Constant-condition folding** — a `cond_br` whose condition is a constant
  (`const_bool`, non-zero `const_i64`, or `const_null`) becomes an unconditional
  `br` to the taken edge; a `switch` on a constant scrutinee folds to its matching
  case. A `while (true)` loop, for instance, lowers to a constant `cond_br` that
  this fold collapses.
- **Empty-block jump threading** — predecessors of an empty, parameterless
  forwarding block (one that only branches onward) are redirected to the end of
  the forwarding chain.
- **Unreachable-block removal** — blocks no longer reachable from the entry are
  neutralized (terminator set to `unreachable`, instructions to `nop`).

Functions that use exception handling are skipped, because their handler blocks
are reachable through implicit edges that the terminator graph does not show.

Later releases add more EIR passes (common-subexpression elimination,
loop-invariant code motion, small-function inlining) to this same driver.

## Register allocation

The EIR backend uses a linear-scan register allocator (Poletto–Sarkar) by
default, keeping hot scalar values in registers across calls instead of spilling
them to the stack on every use.

```bash
# Default: linear-scan registers
elephc hot.php

# Fall back to stack-only placement (spill everything)
elephc --regalloc=stack hot.php
```

`ELEPHC_REGALLOC=stack` applies the fallback to a whole run. The stack fallback
exists mainly for comparison and debugging; linear scan is substantially faster
on compute-heavy code.

## Null representation

`--null-repr` selects how null-capable scalar slots are stored:

| Value | Meaning |
|---|---|
| `tagged` (default) | Inline two-word `{payload, tag}` scalars. |
| `sentinel` | In-band `PHP_INT_MAX - 1` sentinel in one-word slots (legacy opt-out). |

```bash
elephc --null-repr=sentinel legacy.php
```

`ELEPHC_NULL_REPR` overrides the default for a whole run. Most programs should
use the default; `sentinel` exists as a legacy opt-out. See
[Memory Model](../internals/memory-model.md).

## The frozen legacy backend

`--ast-backend` selects the legacy direct AST→assembly backend. It is
**deprecated**, frozen (no new language or runtime features), emits a warning,
and is scheduled for removal in v0.26.0. Use it only to compare behavior with the
old backend during the transition. The EIR backend is the default and the only
active implementation target.
