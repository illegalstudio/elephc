---
id: bug_fix-the-eval-trace-dereferenced-a-handle-it-did-not--d41875cd
type: bug_fix
title: "The eval trace dereferenced a handle it did not own, and aborted the process"
description: "ELEPHC_EVAL_TRACE read three words through a RuntimeCellHandle, but the test implementation mints handles from indices so as_ptr yields 0x1: not null, not aligned, not readable. Three symptoms hid each other -- abort, same abort after a null check, then SIGSEGV after bypassing the precondition. Fixed by moving the peek behind a trait method whose default is None"
created: 2026-09-21
verified_by: "The magician lib suite run with ELEPHC_EVAL_TRACE=all, the configuration that used to kill it: 1750 passed, 8 known-foreign failures, 0 aborts, and a test result line at all"
sources:
  - path: crates/elephc-magician/src/interpreter/runtime_ops.rs
    blob: 0f8ee3b37b052d6dbbc097799370772dc81cceff
    lines: 544-560
    snip: f5453c83c950
    anchor: "fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;"
  - path: crates/elephc-magician/src/interpreter/statements/native_method_execution.rs
    blob: 1cb7e669de9971c681684bd0e646a73992c8075f
    lines: 278-300
    snip: 85c42448b38f
    anchor: "(Ok(()), Ok(())) => {"
---

# The eval trace dereferenced a handle it did not own, and aborted the process

## Fact

A DEBUG FACILITY MUST NOT DEREFERENCE A HANDLE WHOSE REPRESENTATION IT DOES NOT OWN.

`ELEPHC_EVAL_TRACE`'s `native_method_return_contract` line read three words straight through a
`RuntimeCellHandle`. That aborted the whole process — a non-unwinding panic, so SIGABRT with no
backtrace and NO `test result:` line, which reads like an infrastructure failure rather than one
bad read. It fired only with the trace ON, so it was invisible in CI and cost an agent a whole
run it first took for a harness problem.

WHY: `RuntimeValueOps` has TWO implementations. The real adapter's handles are cell pointers; the
test harness mints them from indices (`self.alloc(...)`, `slot as usize`), so its `as_ptr()`
yields values like `0x1` — not null, not aligned, not readable.

THE THREE SYMPTOMS, IN ORDER, each hiding the next:

    original                  abort: "requires the pointer to be aligned and non-null"
    + a null check            SAME abort  -- it was never null, it was misaligned
    + read_unaligned          SIGSEGV     -- it is not readable at all

Rust names both preconditions in ONE message, which is exactly why a null check looks like the
whole fix and is not. And bypassing the precondition check replaced a loud abort with a silent
segfault, which is worse: the check was reporting a real defect, not being fussy.

THE FIX IS ABOUT OWNERSHIP, NOT ABOUT GUARDS. The peek is now a `RuntimeValueOps` method whose
default returns `None`, overridden only by the adapter that owns the representation
(`runtime_hooks/ops/construction_raw.rs`). The real one still reads UNALIGNED, because nothing
promises a `*mut c_void` is 8-aligned and building a `&[u64]` over it aborts rather than returns.
That trait-default-plus-real-override shape is already used elsewhere in this crate.

VERIFIED by running the magician lib suite with `ELEPHC_EVAL_TRACE=all`, which is the
configuration that used to kill it: 1750 passed, 8 failed, 0 aborts, and a `test result:` line at
all. The 8 are a known foreign set (LC family x2, spl_classes, get_loaded_extensions x2, xml,
curl constants, two try/finally).

TWO SMALLER TRAPS MET ON THE WAY:

- `.then(|| unsafe { ... } & MASK)` compiles; `.then(|| { unsafe { ... } & MASK })` does not.
  Inside braces the `unsafe` block is a STATEMENT and the `&` is parsed as a reference:
  "expected `()`, found `u32`".
- A facility that only breaks when switched on is a trap for whoever needs it next. Any test that
  arms a trace must arm it for one thread, never process-wide.

## Why

Rust names both preconditions in one message, so a null check looks like the whole fix; and bypassing the check turned a loud abort into a silent segfault, because the check was reporting a real defect. A facility that only breaks when switched on is invisible in CI.
