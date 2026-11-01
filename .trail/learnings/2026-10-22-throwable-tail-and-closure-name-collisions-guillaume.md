---
title: Throwable payloads must reserve the eval property-hash tail; method closures must never resolve by bare name
author: Guillaume
date: 2026-10-22
branch: reconcile/dirname-symfony
touches:
  - src/types/schema.rs
  - src/codegen/lower_inst/objects/throwable_new.rs
  - src/codegen/lower_inst/objects/fiber_dynamic_entry.rs
  - src/codegen_support/runtime/data/user.rs
  - src/codegen_support/throwable_layout.rs
  - crates/elephc-magician/src/interpreter/reflection/function_construction.rs
  - crates/elephc-magician/src/interpreter/reflection/function_metadata.rs
  - crates/elephc-magician/src/interpreter/tests/dynamic_calls/first_class_objects.rs
  - .plans/symfony-web-execution-ledger.md
---

## Decided

1. **One object payload size authority.** `crate::types::object_payload_size(class_name, info)`
   (in `src/types/schema.rs`) computes `8 + properties*16 + tail`, where `tail` is 8 when the class
   is `stdClass` or `has_property_hash_storage()`. Both the allocators and the
   `_class_object_payload_sizes` table (read by `__rt_object_free_deep`) must use it; do not
   recompute the size from a subset of the flags.
2. **Method closures reflect their method, never a same-named global function.** In
   `reflection/function_construction.rs` and `reflection/function_metadata.rs`, when the closure
   target names a method (`ObjectMethod`, `StaticMethod`, `InvokableObject`, or a `ForeignContext`
   wrapping one), skip the `context.function(name)` / `context.native_function(name)` lookups and
   go straight to the method metadata. Those lookups must only serve `Named`/`BoundNamed` targets.

## Measured mechanism — the secrets UAF

`secrets:reveal` (and `list --reveal`) freed the hash returned by `SodiumVault::list(true)` while
its scope cell still owned it. A hardware watchpoint on the hash refcount (ASLR disabled,
`ELEPHC_EVAL_RC_TRACE` arming) caught the exact stack and the 2 -> 1 transition:

    _rt_decref_hash
    _rt_cleanup_invoke
    _rt_object_free_deep
    _rt_release_slot_table
    elephc_xslice_L_f02e6e3bea60_604   (EnvVarProcessor::getEnv)

`__rt_object_free_deep` reads the dynamic-property tail at
`[object + _class_object_payload_sizes[id] - 8]` and releases it. The freed object was the eval-owned
`EnvNotFoundException` (class id 464): the tables report payload `0x80` and tail flag `1`, but
`lower_builtin_throwable_new` allocated `8 + props*16 + allow_dynamic_properties*8` = `0x78`,
because `reserve_eval_subclass_property_storage` marks every non-final userland class
`eval_property_storage = true` in any program that links the eval bridge. The eight bytes at
`[obj+0x78]` were never zeroed by `emit_allocate` and still held a recycled block's stale pointer —
the `$secrets` payload. Releasing it dropped the hash to 1; the foreach end then released the
iteration copy to 0 and freed the storage the scope still pointed at. That is why the failure was
deterministic, why `--heap-debug` masked it, and why every array-COW hypothesis measured `rc=1` at
the write: the lost owner was gone before the write, inside an unrelated native frame's cleanup.

## Measured mechanism — `debug:twig` yaml_dump

`$this->dump(...)` in `YamlExtension::getFilters()` is a method first-class callable; var-dumper
also defines a global `dump(...$vars)`. Resolving the closure by bare name found the global first,
so `(new ReflectionMethod($cb, '__invoke'))->getParameters()` returned one variadic (`vars`)
instead of the method's three parameters, and `DebugCommand` printed `yaml_dump` with no argument
list. Reproducer: `/tmp/nativeeval/refl9.php` (a global `dump` plus a class method `dump`).

## Evidence

- `/tmp/nativeeval/refl9.php`: 1 parameter (`vars`) before, 3 after.
- `console-fix1/console debug:twig --filter=yaml` -> `yaml_dump(inline, dumpObjects)`.
- Magician suite: 2175 passed, 0 failed, 2 ignored.
- New compiler regression:
  `throwable_payload_size_matches_the_free_layout_for_eval_owned_subclasses`.
- New interpreter regression:
  `execute_program_method_closure_reflection_ignores_a_same_named_global_function`.
- 33-command matrix (`console-oracle.1VqPCG/matrix-fix3` vs
  `console-isolated.zi471L/matrix-fix3`): 31/33 byte-identical; the three remaining differences are
  `$_SERVER['PHP_SELF']` being the absolute compiled entry path, the baked PHP `8.5.0` profile
  versus the oracle's `8.5.10` (+ `Intl locale n/a`), and the wall clock inside `about`.
- Web: 6/6 routes byte-identical in prod (`php-routes-prod` vs `native-routes-prod`,
  `native-routes-preload`, `native-routes-timed`); the generated container is compiled AOT through
  `--ini opcache.preload=<preload.php>` (cold request 0.122 s -> 0.065 s, +53 MB of code).

## Still open

- `highlight_file`/`highlight_string` are not implemented; the dev-mode error page dies with
  `Call to undefined function symfony\component\errorhandler\errorrenderer\highlight_file()`
  (prod responses are unaffected).
- `$_SERVER['PHP_SELF']` uses the canonical absolute entry path (deliberate, so
  `autoload_runtime.php` can `require` it from any cwd) where `php bin/console` reports the invoked
  spelling.
- Single-core web compilation is ~73 s (codegen is 39.8 s of CPU); the 40 s wall figure needs the
  compiler's internal codegen threads. The emitted assembly has no comment volume to strip
  (159 comment lines out of 44 M).
