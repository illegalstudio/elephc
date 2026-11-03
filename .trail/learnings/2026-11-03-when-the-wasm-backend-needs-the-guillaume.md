---
title: When the wasm backend needs the exception-handling proposal
author: Guillaume
date: 2026-11-03
branch: feat/wasm-target
touches:
  - src/codegen_wasm/capability.rs
---

## Decided
- The exception tag is declared by module_uses_exceptions (src/codegen_wasm/function.rs): any try (TryPushHandler, even with no throw), throw, $gen->throw(), Enum::from(), or a read of a property some unset() can clear. Runtime errors alone (division by zero, negative shift) stay direct fatals and declare no tag, so such programs run on hosts without EH.
- The exit tag (module_unwinds_exit) needs a destructor, exit/die outside main, and no fibers; it adds the main landing try_table and the shutdown one (context.rs).
- 'Reactor' in capability.rs is only a validation rule for a module with no is_main function (refuses warnings, TypeError, throw of a message). No CLI path emits a module without _start.

## Tried and ruled out
- Treating 'no explicit raise' as the EH-free subset: a bare try/catch already emits try_table.

## Watch out
- try_table sites beyond the obvious helpers: inst.rs per-call cleanup wrapper that rethrows (the most diffuse), objects.rs drain/release, gc.rs, json.rs, output_buffer.rs, context.rs shutdown, function.rs landings; traps.rs validates them.
