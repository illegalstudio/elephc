---
title: Preserve eval maximum-key effects and ownership through overflow
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1568-eval-max-key-reviews
cost: { minutes: 3490, tokens: 21770922 }
touches:
  - ".plans/eval-array-reference-key-lease.md"
  - ".plans/hash-destructor-insertion-order.md"
  - ".plans/issue-1494-review-followup.md"
  - crates/elephc-magician/src/context/reference_metadata.rs
  - crates/elephc-magician/src/interpreter/array_literals.rs
  - crates/elephc-magician/src/interpreter/array_reference_arguments.rs
  - crates/elephc-magician/src/interpreter/array_references.rs
  - crates/elephc-magician/src/interpreter/builtins/array/array_push.rs
  - crates/elephc-magician/src/interpreter/builtins/registry/dynamic_mutation.rs
  - crates/elephc-magician/src/interpreter/dynamic_functions.rs
---

## Decided

PR1568 keeps initially empty arrays distinct from fresh literals: negative writes into an empty constant append at0 through PHP82, while a fresh negative-key literal continues from its negative cursor on every supported profile. For saturated literal construction, evaluate and own the value before testing whether the implicit key is occupied. Its exception must take precedence over the insertion error. For multi-value array_push, track whether PHP_INT_MAX was actually inserted; an insertion at MAX-1 must not reject the next still-free MAX key. Preserve the inserted prefix when a later value overflows.

Associative replacement construction owns temporary iteration keys and copied values. Retire those leases after insertion retains them, and release the partial result on failure. Direct inserted arguments use with_eval_operands; direct and dynamic replacement dispatch share the context-aware helper.

## Tried and ruled out

Checking MAX occupancy before value evaluation suppresses observable effects. Rejecting MAX merely because an earlier value was inserted loses the last legal slot. Retaining every temporary to avoid an invalid release leaks native Mixed cells. The seven original native review fixtures remain byte-identical and compare full PHP output plus clean heaps. Their independent blockers were fixed by PR1873 (Rust-owned reference keys) and PR1874 (hash destructor insertion order), rather than weakening assertions.

Trail check matched rejected behavior against plans describing its removal. The changes do not restore RuntimeCellHandle keys in persistent targets, flatten Synthetic groups, change heap-kind/resynchronization arguments, or detach property-store pins. Existing learnings stay immutable; the commit/PR explain these lexical matches.

## Watch out

The combined source passed44 interpreter units,49 focused tests per IR mode,28 opaque CLI cases at PHP82/85 and22 profile controls; both Linux Magician checks passed. Keep copied-array acceptance with PR1867 as a separate integration gate and use an emitted function body for iOS static-library probes. iOS Magician's existing pcntl cfg failure is a separate main baseline; assembly is not native execution.

Additional profile probes initially used json_encode(array_keys(...)); JSON iteration and array_keys leaked the same native cells in the older main-reference artifact, without these feature changes. Preserve those failed controls for dedicated main fixes. Print actual keys/values directly with foreach to isolate profile behavior, retaining full-output and clean-heap checks. array_push(array-only) also has an unchanged main arity gate (ec207fda917); do not misclassify it as the MAX overflow regression. Freeze compiler, bridge archives, probes and runtime emitters before switching shared Cargo worktrees.
