---
title: Dynamic enum constant boxing must retain exactly once
author: Guillaumeloulier
date: 2026-10-20
branch: reconcile/dirname-symfony
touches:
  - src/codegen/eval_class_constant_helpers.rs
  - target/framework-audit-20261020.Wcomr7/enum-focused-round5.log
---

## Decided
The enum materializer lends the singleton owned by its global slot. The class-constant getter must box that borrowed pointer with emit_box_current_value_as_mixed, which already retains the object. An additional explicit retain leaks one reference per dynamic constant lookup. Remove it in both emit_aarch64_constant_value and emit_x86_64_constant_value in src/codegen/eval_class_constant_helpers.rs.

## Tried and ruled out
Releasing the singleton roots at terminal CLI shutdown and marking ScopedConstantGet as an owning temporary fixed typed reads, but did not fix dynamic constant lookups. A direct constant read was heap-clean while a runtime string passed to constant left one enum object with one reference. Nested reflected arrays magnified the same duplicate retain; the initial two objects remained with six and three references.

## Watch out
The focused log target/framework-audit-20261020.Wcomr7/enum-focused-round5.log has three passes: closed_world_reflected_enum_case_arrays_match_php, dynamic_enum_constant_boxes_release_their_singletons, and enum_case_roots_are_released_at_shutdown. All assert zero live blocks. The default-mode reflected_enum_case_arrays_match_php still fails because bridge parameter metadata omits compiled default initializers; this is a separate unfixed mechanism, not an accepted baseline. Host tests do not prove Linux or iOS execution. Preserve the singleton root through shutdown callbacks and destructors before retiring it.
