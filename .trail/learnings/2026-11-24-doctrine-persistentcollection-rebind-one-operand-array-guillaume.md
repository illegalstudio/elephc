---
title: "Doctrine PersistentCollection rebind, one-operand array_merge, definedness replays, class-name ladders"
author: Guillaume
date: 2026-11-24
branch: bead-persister
cost: { minutes: 300, tokens: 900000 }
touches:
  - tests/codegen/array_merge_single_operand.rs
---

## Decided
- elephc-hnb1 (Doctrine PersistentCollection::matching refused "CollectionPersister::loadCriteria expects at least 2 arguments, got 1"): the join after `if (c) { $p = A; return …; }` widens the unbound $p's READ view with null (publish_binding_read_views). The next plain store `$p = B` then saw env CP|null, got `null` added to its own type (existing env contains Void), and merged into CP|null|B, so the read typed as the first class. Checker::binding_widened_views remembers (binding, view) per name; check_assign_value_type calls restore_widened_binding_view before computing the stored type, so a plain store merges against the binding the view widened (as before b35f354778). A join that recorded the view as guarded_union_types storage is dropped with it. Reproduced without Demo: red1.php/red2.php style files (interface CP/EP, `$this->getUnitOfWork()->cp()` chain), and the real Doctrine files through a driver project (drv.php + a preload listing the Doctrine src files, ELEPHC_DEFERRAL_MEMO=0, --check, ~90 s release): the earlier "reduction compiles" probes used `$u->cp()` on a parameter, which never made the chain on demand.
- loop_stmt_depth is frame state: type_compat/declarations.rs saves, zeroes and restores it next to break_continue_depth.
- elephc-vjho: array_merge($x) with ONE operand that is not a packed literal (a boxed `array` method/function/property result, a hash, a local) reached the native one-operand helper as a boxed mixed cell: string result, TypeError at the return, or a crash. It was never about ternary arms. lower_gradual_array_merge now rewrites one operand as array_merge($x, []) through the two-operand helper. tests/codegen/array_merge_single_operand.rs covers ?:, match, ??, &&, || arms beside [] and array_values/filter/unique/reverse/keys/slice.
- Definedness replays (item 2): every body with a read the first lowering found possibly unassigned was lowered twice (240 of 248 bodies in a 40-function fixture). The checker already knows: note_possibly_missing_read files a marker (scope, Span::dummy(), name) in local_value_read_types for a read outside a loop whose name is not definitely bound; begin_body tracks those names up front. Replay stays as the safety net. ELEPHC_DEFINEDNESS_STATS=1 prints bodies/replayed/discarded instructions; the counter test is lowering_complexity::maybe_missing_reads_are_flagged_without_lowering_the_body_twice.
- get_class_vars/get_class_methods name ladders were superlinear because each rung lowered `strcasecmp(..) === 0 || strcasecmp(..) === 0` as PHP expressions: ExprBranchJoin::finish joins the type of every local of the body and each rung declared hidden temps, so the join grew with the ladder. 800 classes: 3990 ms -> 521 ms (debug build), per-class IR 40 -> 34 instructions. branch_on_class_name emits the runtime strcasecmp compare directly.

## Tried and ruled out
- Pre-seeding every name written in a conditional: over-tracks (a flag slot and a store per write for names that are always defined).
- Sharing the empty-inventory block of classes with no public property: emit_initialization_deprecations may still emit code for such a class, so emptiness is not known before lowering the match block.

## Watch out
- Pre-existing failures also seen with frozen-mm9 and mm10 (not caused by these changes): codegen test_self_recursive_untyped_method_feeds_array_merge, nullable_array_merge_call_spreads_keep_native_source_shape, nullable_array_merge_inferred_returns, deepclone_shapes closure capture by reference of an unset local, element_unset_regressions throwing key diagnostic, eval_cycles multi operand unset order.
- A `types::checker::driver::tests::enum_metadata_records_abstract_instance_and_static_methods` lib failure appears in this branch and in no earlier baseline log; the baselines never ran the types:: filter, so it is unverified at merge-main.
