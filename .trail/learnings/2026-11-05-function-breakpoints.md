---
title: DAP function breakpoints stop at the enter hook, with the callee frame already readable
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 80 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- `setFunctionBreakpoints` resolves each requested name against the embedded
  function metadata and stores `function id -> optional condition`. Matching is
  case-insensitive and accepts either the fully qualified PHP-visible name or its
  short name; include variants are reduced by `display_function_name`.
- The generated function-enter hook checks the table. An unknown name or a
  malformed condition returns `verified: false` with a message; an empty request
  clears the table and the fast-path atomic.
- A function breakpoint stop happens in the enter hook, before the body runs but
  after the prologue has spilled the parameters. The DAP stop reason is
  `"function breakpoint"`, and the existing scopes/variables/stackTrace surfaces
  inspect the callee frame directly.
- A function breakpoint may carry the same reduced expression as a line
  condition. It is validated at `setFunctionBreakpoints` and evaluated against
  the callee frame at entry; a false condition continues into the body.
- The DAP request loop was extracted from `dap_stopped` into
  `dap_wait_for_commands` so function-entry stops and source-site stops share
  the same stack/scopes/variables/continue/next/step/exit behavior.
- A frame with no source site (a function entry) falls back to the function
  metadata's `source_id`/`line_start` in `stackTrace`, so the top frame still has
  a source location.

## Tried and ruled out

- Checking function names in the site hook: the first source site of a function
  may be skipped by a branch, and a function with no source sites would never
  stop. The enter hook is the semantic entry point.
- Matching only the fully qualified name: PHP names are case-insensitive and IDE
  clients often use the short name; both are accepted.
- Holding the debugger lock for every enter even without function breakpoints:
  the `FUNCTION_BREAKPOINTS_ACTIVE` atomic gates the lock so a normal debug run
  pays nothing.

## Watch out

- Short-name matching can arm a function in several namespaces. The response
  currently reports one id while all matches are stored; `hitBreakpointIds` uses
  the hit function id.
- Method short names are not reduced past `Class::method`; clients should send
  `Class::method` or the fully qualified method name.
- Function-breakpoint conditions have the same reduced expression language and
  limitations as line conditions.
- The stop is before the body, so locals contain parameters/defaults but no
  computed body locals yet.
- The e2e test targets `target()` with `$x === 4`; the first call `target(3)` is
  skipped, the second stops with `$x=int(4)`, the stack is target/calls/main, and
  the final output is `14`.
