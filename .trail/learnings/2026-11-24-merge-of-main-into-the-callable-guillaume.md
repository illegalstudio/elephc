---
title: "Merge of main into the callable branch: eval owners, scope-aware dispatcher, walker tag"
author: Guillaume
date: 2026-11-24
branch: bead-callables
cost: { minutes: 600 }
touches:
  - src/codegen/lower_inst/callables.rs
  - src/codegen/shared_mixed_callable.rs
  - src/codegen_support/runtime/arrays/gc_frame_children.rs
  - src/codegen_support/abi/call_operand_scope.rs
  - src/codegen/eval_argument_helpers.rs
---

Context: elephc-bogc, 33 codegen tests regressed by the merge. Findings that cost time:
1. The harness (tests/codegen/support/compiler.rs) mirrors pipeline::compile by hand. Main's version injects the CLI session surface, so a program that calls an unseen callable compiles the save-handler adapter whose open callback sites share the open dispatcher: tests written for the smaller world (first invoker block, 'one site stays inline', runtime object with no features) break without any compiler change. Follow the closure's own descriptor to its invoker body instead of the first block; use the runtime asm the compile returned.
2. Two owner schemes for eval callable arguments existed after the merge (branch per-argument slot, main argument-owner collection via argument_owners::keep): double release, SIGSEGV. Main's scheme covers throws and suspended Fibers; keep only it.
3. Records published in _exc_call_frame_top come in two layouts: call-operand (visitor at word 5) and PHP activation (caller file at word 5, visitor at word 6). gc_frame_children told them apart by two cleanup symbols; every custom cleanup (bridges, owned-args boundary, invokers, Rust owner frames) was read as an activation. Word 4 now carries CALL_OPERAND_RECORD_TAG; any new creator of the shorter layout must store it (the Rust owner-frame entry does).
4. Main's _eir_callable_argument_normalizer has no lexical class and a fixed message; the branch's open dispatcher (shared_mixed_callable) owns calling-scope visibility and 'call_user_func(): Argument #1 ($callback)' wording. Programs without eval use the dispatcher; the normalizer serves eval programs in functions with no eval context and no non-public reach.
5. A baseline binary of the pre-merge commit (git archive into .elephc-work/base68, cargo build --bin elephc) answered 'did this ever work' in minutes: it cannot link programs (no bridge archives) but --emit-asm and --emit-ir work.
Ruled out: restoring branch's inline path everywhere (main's eval tests need the normalizer's owner discovery).
