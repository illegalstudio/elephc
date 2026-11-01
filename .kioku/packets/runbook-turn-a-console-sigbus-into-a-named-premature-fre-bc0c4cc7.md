---
id: runbook-turn-a-console-sigbus-into-a-named-premature-fre-bc0c4cc7
type: runbook
title: "Turn a console SIGBUS into a named premature free with heap-debug and one lldb regex"
description: "The crash PC is ASCII from a reused block, so the crash site names nothing; --heap-debug plus a regex breakpoint on heap_debug_fail names the caller that freed a still-referenced receiver"
created: 2026-09-22
verified_by: "the compiled Symfony bin/console, about"
sources:
  - path: crates/elephc-magician/src/interpreter/expressions.rs
    blob: 46f8024e2bf651eac3a546b9a5adcee91d39d1c7
    lines: 677-723
    snip: 1d6fcfde165e
    anchor: "fn eval_method_call_with_temporary_receiver_cleanup("
  - path: src/codegen_support/runtime/arrays/heap_free.rs
    blob: dac72fa61dcd0931ef48808036b1eb7057eef9bf
    lines: 110-125
    snip: 9981fab1359e
    anchor: "crate::codegen_support::abi::emit_symbol_address(emitter, \"x16\", \"_heap_debug_enabled\");"
---

# Turn a console SIGBUS into a named premature free with heap-debug and one lldb regex

## Fact

The compiled Symfony console SIGBUSes (`rc=138`) while booting, and the crash site is useless:
lldb reports `EXC_BAD_ACCESS` with

    pc      = 0x0000645f746c7561
    address = 0x636f645f746c7561

Those are ASCII. Decoded little-endian they read `ault_d` and `ault_doc`, and the only string in
the tree that matches is `default_doctrine_dbal_provider`, a DI container parameter name. A block
was freed, the allocator handed the memory to that string, and a later call went through the stale
pointer into string bytes.

FIND THE FREE, NOT THE CRASH. Rebuild with `--heap-debug --keep-symbols` and the runtime says so
before the corruption spreads:

    Fatal error: heap debug detected free of a still-referenced block

and `lldb -b -o "breakpoint set -r heap_debug_fail" -o run -o "bt 25"` names the caller:

    frame #1  _rt_object_free_deep
    frame #2  _rt_mixed_free_deep
    frame #3  interpreter::statements::array_updates::eval_release_value
    frame #4  interpreter::expressions::eval_method_call_with_temporary_receiver_cleanup
    frame #5+ interpreter::expressions::eval_expr  (deeply recursive)

So a method call released its receiver as a disposable temporary while another owner still held
it. `eval_method_call_with_temporary_receiver_cleanup` itself is heavily documented and its own
logic is consistent -- it retains when the result aliases the receiver, then releases when
`receiver_is_temporary`. The defect is therefore UPSTREAM, in whatever classified that receiver as
temporary.

TWO TOOLING NOTES, both cost time:
  - `breakpoint set -n __rt_heap_debug_fail` stays `pending` even though `nm` lists the symbol.
    `breakpoint set -r heap_debug_fail` binds immediately (lldb reports it as `_rt_heap_debug_fail`,
    one underscore short of the assembly label).
  - The binary is stripped by default; `--keep-symbols` is required, and in `zsh` a flag list held
    in a variable needs `${=VAR}` or it is passed as a single word and rejected as
    `Unknown flag: --heap-debug --keep-symbols`.

REACHED, NOT INTRODUCED. The console never booted this far before the eager-file and
declaration-boundary fixes landed; the crash is newly REACHABLE. Raising `--heap-size` past the
8 MB default is what replaces the clean `heap memory exhausted` message with the SIGBUS -- 32 MB
and 256 MB behave identically, so the size is not the variable.

## Why

Two lldb and zsh details in it each cost a rebuild, and the backtrace is the only thing that names the site
