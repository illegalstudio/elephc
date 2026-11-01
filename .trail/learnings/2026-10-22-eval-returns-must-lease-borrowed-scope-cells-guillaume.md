---
title: Eval declared returns must lease borrowed scope cells; array-element writes must not free the receiver
author: Guillaume
date: 2026-10-22
branch: reconcile/dirname-symfony
touches:
  - crates/elephc-magician/src/interpreter/return_values.rs
  - crates/elephc-magician/src/interpreter/scope_cells.rs
  - crates/elephc-magician/src/interpreter/statements/array_updates.rs
  - ".plans/symfony-web-execution-ledger.md"
---

## Decided
`eval_declared_return_control_value` now leases a *borrowed* return value with `values.retain`
before it applies the declared return type, mirroring the `Throw` arm. A `return $var;` hands
back the activation scope's own cell (`EvalControl::Return(value)` carries a borrowed handle),
and `finish_activation_scope` releases every owned local immediately afterwards; without the
lease the caller receives a box the callee is about to free.

## Measured mechanism
Symfony `secrets:list --reveal` (and `secrets:reveal`, `secrets:decrypt-to-local`,
`secrets:encrypt-from-local`) crash with SIGSEGV in `elephc_eval_value_array_iter_key` while
`SecretsListCommand::execute` iterates the array returned by `SodiumVault::list(true)`.
`ELEPHC_EVAL_TRACE=1` plus a temporary trace in `finish_activation_scope` showed the whole graph
is interpreted (`SodiumVault`, `DotenvVault`, `SecretsListCommand` and the generated
`Container3yplunE\App_KernelProdContainer` have no native symbols in `console.s`), and that
`list()`'s activation still owns `$secrets` while the returned clone shares its payload.
By the time the activation drain runs, the payload header (16 bytes before the hash) holds the
allocator's `0xf2ee…` free poison, so `count()`/`foreach` read the free-list pointer
(`4515965296 secrets found in the vault`) or fault boxing the key.

## Still open (do not assume this fix closes it)
The trace still shows the payload freed before the drain in the `list(true)` path, so a second
release exists. lldb watchpoints on the payload header locate it exactly: after the foreach copy
increfs the hash to 2, a native frame cleanup decrefs it back to 1:

    _rt_decref_hash <- _rt_cleanup_invoke <- _rt_object_free_deep
      <- _rt_release_slot_table <- elephc_xslice_L_f02e6e3bea60_604

That slice is `method___Symfony_N_Component_N_DependencyInjection_N_EnvVarProcessor___getenv`,
reached from `reveal()` -> `loadKeys()` -> `LazyString::__toString` -> `Container::getEnv`.
`__rt_release_slot_table` is emitted by `src/codegen/frame.rs:1861` from the slot table built for a
native frame; kind `__rt_decref_object` frees an object whose deferred cleanup
(`_rt_cleanup_invoke`) then decrefs the interpreter's array. So a native frame owns a borrowed
reference to an interpreter-owned array. Naming that object (read its class id / pending cleanup
list at `_rt_object_free_deep`) and finding where eval handed it the array is the next step.

Reproduce with: `--heap-debug` masks it, `settings set target.disable-aslr true` is required,
break on `__rt_mixed_array_set` by address after `main` is reached, filter `tag==5 && key length
23`, watch `payload-12`.


## Watch out
`--heap-debug` masks the fault completely (different allocator layout), so a green heap-debug run
is not evidence. `settings set target.disable-aslr true` makes heap addresses reproducible across
lldb runs of the same binary, which is required to set a watchpoint on a payload header.
The generated Symfony container is created by `cache:warmup`/`cache:clear` at runtime, so the whole
application runs through eval today: compiling that generated container ahead of time is the single
largest reduction of eval surface for the framework campaign.
