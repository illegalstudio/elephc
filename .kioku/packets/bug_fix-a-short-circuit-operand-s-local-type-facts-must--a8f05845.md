---
id: bug_fix-a-short-circuit-operand-s-local-type-facts-must--a8f05845
type: bug_fix
title: "A short-circuit operand's local-type facts must be JOINED at the merge, not restored"
description: "Why an assignment inside the right operand of && was silently discarded, and what it cost"
tags: [ir_lower, symfony]
created: 2026-09-16
verified_by: "scratchpad/retypecond.php and retypecond2.php byte-identical against php -n"
sources:
  - path: src/ir_lower/expr/unary_logic.rs
    blob: 20102bb66d26f30482f221a953f750bdb6a6d410
---

# A short-circuit operand's local-type facts must be JOINED at the merge, not restored

## Fact

`lower_logical_binary` snapshotted `local_types` before the right operand and restored it wholesale
afterwards. The snapshot exists to undo the instanceof NARROWING applied to the operand
(`$r instanceof R && $r->m()`), but it also undid every real ASSIGNMENT the operand made.

PHP puts stores there routinely, because `&&` short-circuits and the store cannot be hoisted:

    if (!($filter & Caster::EXCLUDE_VERBOSE) && $v = $c->getStaticVariables()) {
        foreach ($v as $k => &$v) { … }

`$v` had been the element of an earlier `foreach ($c->getParameters() as $v)`, so the restore put
`ReflectionParameter` back as the logical fact while the store had already widened the slot. The
`foreach` below was then lowered as an iteration of that class:
`unsupported EIR backend feature: iterator method ReflectionParameter::rewind`.

`join_short_circuit_operand_types` restores the saved facts, then, for every name the operand
changed, re-reads the FRAME SLOT's storage type — the join every store already widened it to, and
the same contract `join_arm_types` gives an `if`. A narrowing leaves the slot untouched so it comes
back to the declared type; an assignment comes back as the widened storage. One rule, both cases.
