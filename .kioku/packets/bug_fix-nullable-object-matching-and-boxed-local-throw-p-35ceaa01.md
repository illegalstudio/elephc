---
id: bug_fix-nullable-object-matching-and-boxed-local-throw-p-35ceaa01
type: bug_fix
title: "Nullable object matching and boxed local throw preserve native ownership"
description: "Scoped compiler checkpoint: normalize object null before matcher; move original Mixed throw box without early decref. Native heap regression and five-target EIR emitter gates pass."
created: 2026-10-12
verified_by: "cargo test -p elephc --features curl --lib mixed_throw_errors: 6/6; codegen throw_nullable_object_local_preserves_exception_box: 1/1 heap clean; codegen test_instanceof_null_catch_assigned_object_local final: 1/1. PHP oracle retained and FFTT. Logs /tmp/elephc-task-{throw-box-targets,throw-box-regression,instanceof-null-final}-20261012.log."
sources:
  - path: src/ir_lower/ownership.rs
    blob: ec7db9f87505644a75a1c8108ac23fd047268ff8
    lines: 78-117
    snip: 4100034c3ea5
    anchor: "pub(crate) fn take_direct_local_load("
  - path: src/codegen/lower_inst/objects/instanceof_helpers.rs
    blob: 2ff67b9918a97c062bc2d7513e6d346a4daa0dee
    lines: 13-47
    snip: 82c10fe6b61d
    anchor: "pub(super) fn emit_normalized_dynamic_instanceof_value("
  - path: src/codegen/lower_inst/objects/instanceof_entry.rs
    blob: 32ab65a24b4c5111206158e508871f994e5332d6
    lines: 37-48
    snip: 99e50e5ef0cf
    anchor: "match value_ty {"
  - path: src/ir_lower/tests/mixed_throw_errors.rs
    blob: 14352cc19124eeadba9c019d948f4c1918b50d3a
    lines: 11-35
    snip: 5348ab8c1d15
    anchor: "fn nullable_throw_box_transfer_on_all_supported_targets() {"
  - path: tests/codegen/exceptions.rs
    blob: c398ea72fbfaf4997713742eb53357ca16895a66
    lines: 11-26
    snip: 23b8d94e6aab
    anchor: "fn test_throw_nullable_object_local_preserves_exception_box() {"
  - path: tests/codegen/oop/instanceof.rs
    blob: 3672419f7c70bd13fa34de8555d8a2ba962f783b
    lines: 145-161
    snip: 9b10399c63b5
    anchor: "fn test_instanceof_null_catch_assigned_object_local() {"
---

# Nullable object matching and boxed local throw preserve native ownership

## Fact

Object slots can carry zero or NULL_SENTINEL; normalize before both named and dynamic instanceof matcher entry. take_direct_local_load releases a physical Mixed source only when a concrete payload was separately extracted. A Mixed-to-Mixed throw moves the original box by zeroing the slot, without prior release. Focused native and five-target emitter controls pass; no full scheduler, Linux native, heartbeat or exact-artifact acceptance is claimed.
