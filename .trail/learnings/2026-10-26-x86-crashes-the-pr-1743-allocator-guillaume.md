---
title: x86 crashes the PR 1743 allocator exposed
author: Guillaume
date: 2026-10-26
branch: pr-1743-r2
touches:
  - src/codegen/lower_inst/builtins/arrays/misc_dispatch.rs
  - src/codegen_support/runtime/arrays/heap_free.rs
---

## Decided
- x86_64 runtime release helpers (__rt_decref_hash and friends) take their pointer in rax, not rdi. lower_two_hash_arg_builtin loaded the converted temporary into rdi, so it released the RESULT (still in rax) and leaked the temporary. Fixed to mov rax.
- x86_64 __rt_heap_free_safe now skips refcount-0 blocks and implausible sizes, as AArch64 always did. The IR double-releases a dynamic property name string on C::$o->$name->v = f(); AArch64 tolerated it, x86 reported a double free.
- Both bugs were on main; the exact size-class bins of PR 1743 reuse a freed block at once, which turned silent use-after-free into SIGSEGV on linux-x86_64 CI only.

## Tried and ruled out
- The mixed-array ref-marker writeback and the itoa spill ring (both touched by the PR) were not the cause.

## Watch out
- Under the Docker x86 image (qemu-user), a segfault prints 'qemu: uncaught target signal 11' and the container then hangs; it looks like a hang. Debug with QEMU_GDB=1234 ./prog & then gdb 'target remote :1234' in the same container, building with --keep-symbols.
- php_ext_tests must read CARGO_BIN_EXE_elephc / CARGO_MANIFEST_DIR at run time: CI runs an extracted nextest archive where env!() build paths do not exist.
