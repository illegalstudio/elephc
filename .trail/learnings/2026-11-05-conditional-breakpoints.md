---
title: Conditional breakpoints evaluate a tiny side-effect-free expression before each stop
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 90 }
touches:
  - crates/elephc-debug/src/condition.rs
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- DAP breakpoints carry an optional `condition` string. `setBreakpoints`
  validates its syntax before arming the site; a malformed expression returns
  `verified: false` with the parser error and does not arm.
- Conditions are stored per site id and evaluated against the **current hit
  frame** before the breakpoint stop. A false result continues execution; a true
  result stops with `reason: "breakpoint"`. This is necessary because a client
  normally sets breakpoints at the entry stop, before the target function's
  locals exist.
- The evaluator is a small recursive-descent parser over a reduced PHP scalar
  language: int/float/string/bool/null literals, `$name`/bare locals,
  parentheses, `!`, `&&`, `||`, comparisons and `+ - * / %`. It reads locals
  through the same borrowed value reader as DAP `variables`; it never calls PHP
  or allocates in the debuggee.
- A runtime failure (unknown local, non-numeric string in arithmetic, division by
  zero) is sent to the DAP client as an `output` event; that hit is skipped rather
  than silently stopping or aborting the program.
- Arrays, objects, resources and callables are `Other`: truthy only, and not
  arithmetic/comparison operands. Strict `===` requires the same scalar type;
  loose `==` follows reduced PHP scalar rules.

## Tried and ruled out

- Evaluating the condition only at `setBreakpoints`: a breakpoint is usually
  configured at the entry stop, where the target function's locals do not exist.
  Syntax validation happens there; truth is evaluated at each hit.
- Calling the PHP parser/compiler from the debuggee: the runtime has no AST, and
  compiling/evaluating PHP in the stopped frame would mutate the debuggee, run
  user code and invalidate borrowed inspection.
- Stopping on a condition error: that would make a typo look like a hit. The
  client receives an `output` event instead, and execution continues.

## Watch out

- The expression language is intentionally smaller than PHP: no function calls,
  property access, array indexing, string interpolation or short-circuit `&&`/`||`.
  Both sides of a boolean operator are currently evaluated.
- Empty arrays are `Other` and therefore truthy in this reduced evaluator; PHP
  would treat an empty array as false. Do not rely on array truthiness yet.
- Division always returns a float; strict `4 / 2 === 2` is not PHP-equivalent.
- The condition is evaluated while the debugger mutex is held, against the
  innermost activation. Generator/fiber stacks inherit the existing activation
  limitations.
- The e2e test sets `$i === 3` at the entry stop, lets `i=0..2` skip, stops at
  `i=3`, verifies `$i=int(3)`/`$total=int(3)`, continues past `i=4`, and checks
  the final output `10`. A malformed `$i ===` is rejected as unverified.
