---
id: bug_fix-an-autoloaded-file-s-top-level-return-deleted-91-6fec6999
type: bug_fix
title: "An autoloaded file's top-level return deleted 91% of the program"
description: "autoload::load_autoloaded_file splices a file's statements into the entry program's top level without restoring php's include boundary, so a file-scope return became statement 160 of 1848 and optimize::propagate deleted the 1688 that followed, including the whole error-handling prelude. The only symptom was call to unknown function error_log, four passes and one unrelated function away. ELEPHC_DECL_TRACE finds this class in one build"
created: 2026-09-21
verified_by: "ELEPHC_DECL_TRACE=error_log on the Symfony --web build; three framework-free repros under ten lines each; five mutations each killing exactly its intended test"
sources:
  - path: src/autoload/mod.rs
    blob: ff1b8475f953bf956eb5cc515926c967970040e8
  - path: src/pipeline.rs
    blob: 6c7702ffc7c77c81fd120accd4d454343e7b96c4
---

# An autoloaded file's top-level return deleted 91% of the program

## Fact

An autoloaded file whose top level ends in `return` DELETED 91% OF THE PROGRAM, silently, and the
only symptom was an unrelated function name four passes later.

THE CHAIN, measured with `ELEPHC_DECL_TRACE=error_log` in a single 60-second build:

1. A Composer eager `autoload.files` entry ends with `return require __DIR__.'/bootstrap72.php';`.
2. `autoload::load_autoloaded_file` splices that file's name-resolved statements **straight into
   the entry program's top level**. `crate::resolver` has restored php's include boundary for a
   written-out `require` since #529 (`discard_statement_include_return`); the AUTOLOAD path never
   did. So a file-scope `return` became top-level statement 160 of 1 848.
3. `optimize::propagate`'s `propagate_block` stops at the first statement that does not fall
   through, and deleted **1 688 top-level statements**, including the entire `--web`
   error-handling prelude. `check_result.functions` still held them.
4. `ir_lower` therefore saw `error_log` as a user function and emitted `Op::Call`; codegen had no
   body. The error read `call to unknown function error_log (in __elephc_filter_var_dyn_arr)` —
   naming a function nobody had touched, in a helper unrelated to the cause.

THE FIX: `discard_autoloaded_file_return` degrades a top-level `return E;` in an autoloaded file
to `E;` — the expression still runs, because `return require 'x.php'` performs the include — and
drops the unreachable executable tail. **Declarations after the `return` are KEPT**: php binds
unconditional top-level declarations at file-compile time, verified as `<?php return 1; function
f(){}` leaving `function_exists('f')` true in the includer.

THE DIAGNOSTIC THAT FOUND IT, now permanent: `ELEPHC_DECL_TRACE=<function>` in `src/pipeline.rs`,
the function-side twin of `ELEPHC_METADATA_TRACE`. It prints the declaration's survival at each
phase boundary, so "which pass dropped it" is one build rather than a bisect. **When a backend
error names a function nobody touched, reach for this first.**

THE GENERAL LESSON, which is why this is worth reading even if the bug is gone: a backend error
that names an unfamiliar symbol is usually the FIRST SURVIVOR of a deletion, not the defect. Ask
what else disappeared before believing the name in the message.

A SECOND, SEPARATE TRAP AT THE SAME SPOT: do not fold a compile-time-constant `if` around such a
`return` in `opt-fold`. By that phase every autoloaded file is one flat top level, so folding
re-creates exactly this catastrophe. The fold belongs inside `load_autoloaded_file`, BEFORE the
splice, where the `return` still terminates only its own file.

TWO MORE DEFECTS FOUND ON THE SAME PATH, both framework-free and both worth knowing:

- `preg_match`/`preg_match_all` refused a matches destination that already held a string. The fix
  is a storage widening to boxed `Mixed` BEFORE the arguments plus a deferred logical-type install
  AFTER them, because in real code the same name is often the SUBJECT and the destination of the
  same call and must still read as a string one operand earlier. A checker-side retype would be
  wrong.
- `$dp = [[0,0],[0,0]]; $dp[1][0] = 5;` failed to compile **inside a function** and compiled fine
  at top level, which is why every existing nested-write fixture missed it. A local typed
  `array<array<int>>` must be normalized to `array<mixed>` before a nested write, because the
  nested writer finishes through the boxed-Mixed path.

WHY THE EAGER FILES ARE WORTH THE QUEUE: three of the four defects hit on this path reproduce in
under ten lines of framework-free PHP and would bite any program. Compiling Composer's eager
files acts as a cheap fuzzer over ordinary language surface, and the loop is fast — 38 to 172
seconds to the next error.

## Why

A backend error naming an unfamiliar symbol is usually the FIRST SURVIVOR of a deletion, not the defect. Ask what else disappeared before believing the name in the message.
