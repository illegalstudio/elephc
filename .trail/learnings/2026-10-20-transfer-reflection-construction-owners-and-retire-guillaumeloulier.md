---
title: Transfer Reflection construction owners and retire metadata leases
author: Guillaumeloulier
date: 2026-10-20
branch: fix/eval-reflection-owner-boxing
touches:
  - crates/elephc-magician/src/interpreter/reflection/class_lookup.rs
  - crates/elephc-magician/src/interpreter/reflection/class_member_api.rs
  - crates/elephc-magician/src/interpreter/reflection/constant_construction.rs
  - crates/elephc-magician/src/interpreter/reflection/owner_materialization.rs
  - crates/elephc-magician/src/interpreter/reflection/parameter_construction.rs
  - crates/elephc-magician/src/interpreter/statements/native_static_dispatch.rs
  - src/codegen/eval_reflection_owner_helpers.rs
  - src/codegen/lower_inst/objects/reflection/member_properties_emit.rs
  - src/codegen/lower_inst/objects/reflection/parameter_property_emit.rs
  - src/codegen/lower_inst/objects/reflection/string_attrs_emit.rs
---

## Decided
Fresh AOT Reflection child objects and metadata hashes transfer their initial reference with emit_box_current_owned_value_as_mixed. The native-to-eval factory also retires its raw object reference after __rt_mixed_from_value retains it. Keep the boxed result safe in a consumed-input spill across that release on both architectures.

Reflection string-property replacement clears the old slot and retires its string before persisting the replacement. Generic and specialized metadata stages can write the same name twice. The uniform decref helper guards null and non-heap/uninitialized pointers on both architectures.

Runtime array_set retains its key and value. Reflection metadata builders must retire their own temporary indices, keys and child-object cells, including failure cleanup. Method-parameter target extraction and string-array decoding likewise retire array_get results after copying names into Rust. Reflected constant construction uses finish_eval_argument_values to preserve the new reflector and retire the constant lease. Reflection constructor errors reuse eval_throw_builtin_exception for message/code leases.

## Tried and ruled out
Fake RuntimeValueOps reflection tests alone did not expose the native leaks: 77 unit tests passed while native fresh-process heap checks failed. Use repeated native construction and verify exact output, including getter results used after the parent is unset.

Owned boxing alone reduced the AOT strlen ReflectionParameter reproducer from fourteen live blocks to two strings per construction. Duplicate string writes were an independent source of leaks. The N=1,10,50 native witness must reach zero; the historical public-name test now requires a clean heap instead of accepting a baseline-plus-seven allocation offset.

## Watch out
Do not globally replace borrowed boxing. Shared enum singletons, live static-property values and static callable descriptors keep their borrowed convention. Source fixtures, frozen compiler/bridge artifacts, full stdout and heap diagnostics are separate evidence.

Pristine native enum Reflection metadata independently returns an empty ReflectionEnum name and rejects a valid ReflectionEnumUnitCase. Preserve those witnesses separately; this ownership fix does not establish enum Reflection parity.
