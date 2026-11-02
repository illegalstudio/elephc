---
id: gotcha-not-every-shared-codegen-cache-is-a-lookup-some--f74a1f68
type: gotcha
title: "Not every shared codegen cache is a lookup: some hold module-wide state that must not be split"
description: "Letting all caches be reached from workers linked fine and rendered the 404 as a raw exception dump; the static-method descriptor cases accumulate a dispatch table"
created: 2026-09-20
sources:
  - path: src/codegen/shared_state.rs
    blob: 925e6d4086c2a342f798777339bad4a074872f62
    lines: 300-340
    snip: 41cb4bb19a6d
    anchor: "self.mixed_string_sharing[mode_index]"
  - path: src/codegen/lower_inst/callables.rs
    blob: fe23cb832a644848b55baeaa56a38703fb2490a2
    lines: 2480-2530
    snip: 9ee2b0b501e2
    anchor: "let class_name = class_name.to_string();"
superseded_by: "gotcha-full-parallel-codegen-is-slower-than-the-deferri-e9438af3"
superseded_why: "The diagnosis it recorded was a guess that does not survive reading the code: runtime_static_method_descriptor_case and its plural sibling are pure lookups keyed by their argument, so splitting them across workers duplicates entries rather than partitioning a table. The 404 divergence it blamed on them was never traced. What replaced it is the measurement that makes the question moot: full parallelism is slower than the hybrid."
---

# Not every shared codegen cache is a lookup: some hold module-wide state that must not be split

## Fact

The parallel body pass gives each worker a `SharedCodegenState` of its own. That is safe for a
cache that is a LOOKUP — two workers each emitting a copy of the same helper is duplication, and
once the helper's symbols come from its cache key the merge collapses the copies. It is NOT safe
for a cache that is module-wide STATE.

Marking every family parallel-eligible produced a build that assembled and linked, with
`deferred_share=0.0%` and no serial tail at all — and then served the 404 page as

    Symfony\Component\HttpKernel\Exception\NotFoundHttpException {#1298
      #message: "Unable to find the controller for path "/no-such-route"..."

instead of Symfony's HTML error page. Seven routes byte-identical, one not. Nothing in the
assembly was duplicated or missing; a dynamic callable simply did not match, because
`cache_runtime_static_method_descriptor_case` (`lower_inst/callables.rs:2526`) accumulates a
dispatch TABLE of every static-method case in the module. Eight workers each built a partial
table.

**The test for whether a family may go parallel is not "are its labels keyed" but "is a partial
copy of this cache still correct".** A lookup keyed by a semantic key: yes. An accumulating
registry read as a whole later: no — it has to be precomputed before the workers start, or its
bodies stay serial.

Also learned here: the boundary form of the callable invoker (`catch_native_throws`) is NOT
cached and keeps a host-scoped symbol, so it must not be wrapped as a keyed helper. Giving it the
same key as the shared form made the merge treat two different invokers as copies of one and drop
a definition the data section still referenced —
`"_eir_ReflectionFunction__invo_callable_invoker_...", referenced from: _data_s1_39646`.

The cheap oracle for both classes of mistake is the same one: every Symfony route byte-compared
against `php -S`. The duplicate-symbol and missing-symbol failures are loud; the partitioned
table is not, and only the byte comparison caught it.

## Why

Keying a helper's labels makes duplication safe; it does nothing for a table that has been partitioned
