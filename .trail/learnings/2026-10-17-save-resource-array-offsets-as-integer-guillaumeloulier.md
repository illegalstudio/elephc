---
title: Save resource array offsets as integer reference keys
author: Guillaumeloulier
date: 2026-10-17
branch: fix/eval-array-reference-key-lease
cost: { minutes: 3538, tokens: 21969682 }
touches:
  - crates/elephc-magician/src/interpreter/array_literals.rs
  - tests/codegen/eval/array_reference_keys.rs
  - ".plans/pr1873-resource-key-review.md"
  - ".plans/eval-array-reference-key-lease.md"
  - crates/elephc-magician/src/context/reference_metadata.rs
  - crates/elephc-magician/src/interpreter/array_reference_arguments.rs
  - crates/elephc-magician/src/interpreter/array_references.rs
  - crates/elephc-magician/src/interpreter/dynamic_functions.rs
  - crates/elephc-magician/src/interpreter/mod.rs
  - crates/elephc-magician/src/interpreter/persistent_references.rs
---

## Decided

Persistent eval array keys must accept EVAL_TAG_RESOURCE and save its integer ID through eval_int_value, the same cast already supported by the runtime. The old helper returned None after a successful native lookup, so call arguments aborted before entering the callee. Reuse the shared scalar cast and its temporary-result release. Do not retain a native resource cell in the saved target: the integer key must survive stream closure and resource-variable rebinding.

## Tried and ruled out

Dropping the key-validation guard only for value calls leaves reference calls, nested receivers and retained aliases broken. Keeping a resource handle reintroduces the lifetime coupling that Rust-owned reference keys remove. Reading resource descriptor memory directly duplicates an existing runtime conversion. Four PHP82/85 fixtures cover direct and nested value calls, a reference callee that closes/rebinds the resource, and a reference used after closing the stream. All four fail before the fix; all previous native reference fixtures remain intact.

## Watch out

The review was posted on PR1568 but the helper originates in PR1873, so update both existing fork branches. The shared normalizer also serves reference writes, unset, unpacking, cloning and target equality. Keep its resource conversion quiet; internal metadata comparisons must not create observable offset warnings. Native ordinary resource-offset lookup already omits PHP's warning, independently of this regression. Tests use error_reporting(0), still compare complete output and require clean heaps, and were proven failing under that setting before the fix.

The PR1873 source passes95 focused reference units and three native regression tests per IR mode, using a frozen compiler with newly built Magician and pinned bridge archives. The native regression source is compiled by a std-only adapter against CLI-built opaque-eval programs, avoiding relinking a frozen root rlib after another worktree changes shared Cargo metadata. Preserve the compiler, source hashes, static archives and adapter receipts.
