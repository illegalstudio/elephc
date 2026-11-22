---
title: Merge regressions in eval: result clobbered by the binding clear, unspelled superglobals flushed, PCNTL closure identity, invoker-slot owner
author: Guillaume
date: 2026-11-22
branch: bead-evalglobals
touches:
  - src/codegen/lower_inst/builtins/eval/calls.rs
  - src/codegen/lower_inst/builtins/eval/sync_inventory.rs
  - crates/elephc-magician/src/context/runtime_state.rs
  - crates/elephc-magician/src/interpreter/pcntl_escape.rs
  - crates/elephc-magician/src/interpreter/statements/reference_writeback.rs
---

## Found
- elephc-g7b0 had three independent causes. (1) `lower_eval_literal_scope_eir_function` parked the fragment's result in `EVAL_TEMP_CELL_OFFSET` (the merge changed it from `EVAL_STATUS_SAVE_OFFSET`, which collides with the reload throwable at 88), but `clear_native_scope_bindings` uses that same slot as scratch for each entry's value view. The result became the global's value box and was released: a string written to a `global $g` bound name was freed under the cell (garbage after the function returned; ints survived by luck). The result now has its own `EVAL_RESULT_SAVE_OFFSET` slot beyond the guard record. This also fixed the conditional `global` test.
- (2) The session prelude's position-less functions read and write `$_SERVER`/`$_ENV`, so they made those names program globals and `eval_sync_global_type` flushed the never-initialized storage as an empty entry; the interpreter only seeds a superglobal the scope lacks. Instructions with a line-0 span no longer count for the CLI superglobals off-web. Do NOT fix it by seeding in the compiler: the compiled seed's PHP_SELF is the entry script, the interpreter's is argv[0], and the test (and main) expect `PHP_SELF === argv[0]`.
- (3) PCNTL escape detection keyed foreign callables by the cell pointer of `pcntl_signal_get_handler`'s result. A native sync (`$GLOBALS[...]` write hooks) re-boxes the closure into a fresh cell, so the lookup missed and the fatal never fired (all four `pcntl_detached_eval_handler_cannot_escape_*` tests). The object-arm of `value_contains_foreign_pcntl_callable` now also matches the pinned closure object's identity.
- `array_merge_recursive over objects` (misc::globals_array...) was already fixed by 61d0d40de6.
- elephc-awkt: `written_value_is_owned_by_entry` answered false for `InvokerSlot`, so `$value = "ref"` inside an eval closure called from compiled code with a by-ref argument left the statement's reference unowned; every InvokerSlot write-back arm retains its own copy (or takes a raw word), so the entry owns the reference. Heap-debug repros: `function b(mixed $cb, int|mixed $x){$cb($x);}` with `function(&$v){$v="ref";}` leaked the string and its box (and an int box for `$v = 7`).

## Ruled out
- The invoker/IR side: the IR matched the baseline up to acquire/release elision, and a native by-ref callee (`b('loc', 5)`) was clean.
- Debugging recipe that worked: `--keep-symbols` build, lldb batch with `breakpoint command add` printing x0/pc/lr on `_rt_decref_mixed`/`_rt_incref`/`_rt_heap_free_safe` (lldb strips one leading underscore: `_rt_...`), and the bridge libs need `cb.sh build --workspace` before the CLI can link.
- The merged `__elephc_eval_report_runtime_fatal` only worded statuses 1 and 5, so the escaping-PCNTL status (6) exited silently once the bridge is linked (the constant text exists only for bridge-less programs). It now prints "Fatal error: PCNTL handler/closure cannot escape its eval context".
