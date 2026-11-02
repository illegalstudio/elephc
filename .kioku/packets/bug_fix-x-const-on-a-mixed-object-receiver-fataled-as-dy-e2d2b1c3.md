---
id: bug_fix-x-const-on-a-mixed-object-receiver-fataled-as-dy-e2d2b1c3
type: bug_fix
title: "$x::CONST on a mixed OBJECT receiver fataled as dynamic::CONST and over-released the local"
description: "class_name_to_id only resolved a boxed STRING; a boxed object answered -1 and fell to the eval bridge. The receiver release also decref'd a borrowed load_local, freeing the loop variable"
created: 2026-09-26
sources:
  - path: src/codegen/lower_inst/objects/class_name_id.rs
    blob: b4f531eb557498fba54b05abc9cccce981948db9
  - path: src/ir_lower/expr/scoped_values.rs
    blob: fc0463b63beb54f38e7bc8cd04d934f87dfda699
  - path: src/types/checker/method_pass.rs
    blob: 74980c5aa1150085cbeec7cafd2e7d226ac4b840
---

# $x::CONST on a mixed OBJECT receiver fataled as dynamic::CONST and over-released the local

## Fact

Symptom: every --web route whose controller takes an argument died with
"Undefined constant dynamic::IS_INSTANCEOF" (ArgumentResolver reads $metadata::IS_INSTANCEOF off
the loop variable). It appeared when method_pass's method_overridden_by_descendant guard kept the
interface ArgumentMetadataFactoryInterface::createArgumentMetadata(): array contract instead of
narrowing it to array<ArgumentMetadata>, so $metadata became mixed.

Two independent defects on the mixed path, both needed for the reduction to pass:
1. Op::ClassNameToId on a Mixed operand unboxed and accepted only tag 1 (string). A tag 6 (object)
   now loads the class id from the object's first header word (arm64 and x86).
2. lower_dynamic_scoped_constant called release_if_owned on the receiver unconditionally. The
   codegen Release decrefs any maybe_owned value, so a plain local's box lost a reference per read.
   Symptom: the FIRST pushed string of `$names[] = $m->getName() . ':' . $m::K` showed the second
   iteration's bytes. Now guarded with value_is_owning_temporary, like receiver temporaries.

Rule: release_if_owned does not look at ownership; any lowering that releases an operand must
first ask ctx.value_is_owning_temporary(value).
Pin: tests/codegen/oop/constants.rs test_class_constant_through_a_mixed_object_receiver.

## Why

It is the real reason packet dca80296 said widening an object payload to mixed breaks Symfony's ArgumentResolver; with it fixed, the mixed path is merely slower, not wrong
