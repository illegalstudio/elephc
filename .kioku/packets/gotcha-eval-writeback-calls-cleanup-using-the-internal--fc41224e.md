---
id: gotcha-eval-writeback-calls-cleanup-using-the-internal--fc41224e
type: gotcha
title: "Eval writeback calls cleanup using the internal runtime ABI"
description: "Windows scope retirement must stage RDI RSI RDX rather than native RCX RDX R8"
tags: [windows, abi]
created: 2026-10-11
stale_when: "Cleanup helper convention or generated scope writeback boundary changes"
sources:
  - path: src/codegen/lower_inst/builtins/eval/scope_reload.rs
    blob: 129b2d61533ab381538ccb56338375f4c0931328
  - path: src/codegen_support/runtime/exceptions/cleanup_call.rs
    blob: 3911e68831a17c6031623fdc53a8c9d76e67554c
---

# Eval writeback calls cleanup using the internal runtime ABI

## Fact

Generated eval scope writeback invokes __rt_cleanup_invoke directly; this is an internal runtime call, not a Rust C export. Use runtime_helper_int_arg_reg for function, payload, and pending slot. A live PE fault showed native RCX held the correct decref callback while the helper captured a stack address from RDI as ENTRY. Do not change the shared helper ABI or add a global C adapter to hide a wrongly staged generated caller.
