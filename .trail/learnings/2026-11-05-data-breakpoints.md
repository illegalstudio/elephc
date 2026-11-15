---
title: Data breakpoints are rendered-value snapshots checked at site transitions
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 90 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- `dataBreakpointInfo` resolves a frame-local name from a `VariableContainer::Frame`
  and returns a stable compile-local `dataId` (`local:<function-id>:<name>`),
  `accessTypes: ["write"]` and `canPersist: false`.
- `setDataBreakpoints` accepts only write watches. It validates the dataId and
  local in the debug metadata, snapshots the current rendered value from the
  active frame, then stores the watch.
- The site hook checks watches for the current function at each source-site
  transition. A changed rendered value stops with `reason: "data breakpoint"`.
  This detects scalar and string writes; arrays/objects report their header
  (`array(N)` / `object(C) (N)`) and a mutation inside the container may not
  change that header.
- The watch stores the function id and local name, not a frame address; it is
  therefore reusable across activations of that function. The current activation
  supplies the frame used for each comparison.
- Object-property watches return `dataId: null` with an explanation; reading an
  arbitrary object address later would need a lifetime/ownership proof the
  debugger does not have yet.

## Tried and ruled out

- Checking data breakpoints at every instruction: only source-site guards are
  emitted, and per-instruction instrumentation would be a much larger runtime
  change. The stop is therefore at the next line transition after a write.
- Storing a raw local frame address: it becomes stale when the frame returns or
  the stack is reused. The function/local pair plus the current activation is
  safer.
- Watching object properties by address: the object can be freed or moved, and
  the debugger has no borrowed-lifetime proof for that address. Refused for now.
- Using the PHP parser for the watch expression: data breakpoint info identifies
  a concrete local slot, not a general expression.

## Watch out

- Arrays and objects are watched by their rendered header, not by a deep
  fingerprint; element/property mutations can be missed. A recursive fingerprint
  is the next step if watch-all-changes on containers is needed.
- Recursive activations of the same function share one watch snapshot. Only the
  innermost frame is compared at a site; the watch can miss a change made in a
  parent activation while a child is running.
- The stop happens at the next source-site transition, not at the exact
  instruction that wrote the value.
- Only write access is supported; read/read-write watches would need load hooks.
- The e2e test watches `$x` in `countdown`: at the line-4 breakpoint it snapshots
  `int(0)`, continues past `$x=1`, stops at line 5 with `reason: "data breakpoint"`
  and `$x=int(1)`, clears the watches and the line breakpoint, and finishes with
  output `3`.
