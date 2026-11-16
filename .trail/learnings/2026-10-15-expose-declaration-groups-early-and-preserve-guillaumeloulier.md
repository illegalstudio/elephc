---
title: Expose declaration groups early and preserve lowered statement shapes
author: Guillaumeloulier
date: 2026-10-15
cost: { minutes: 0, tokens: 0 }
touches:
  - src/parser/declaration_groups.rs
  - src/parser/stmt/mod.rs
  - src/name_resolver/mod.rs
  - tests/codegen/types/declare_groups.rs
---

## Decided
Expose groups containing declarations while building file/namespace statement lists, before function discovery; reuse the same idempotent rule before name resolution for transformed programs. Preserve ordinary Synthetic groups. Their shape participates in checker specialization for nested appends and in source-map reset boundaries.

The shared parser helper retains statement metadata and namespace bodies and does not descend into expression-owned closures. Earlier callable bodies retain their lexical weak profile even when a later leading strict directive changes the main op-array's mode.

## Tried and ruled out
Flattening every root Synthetic group made a shared indexed bucket fail count() checking and emitted an undefined-key warning during associative autovivification. It also made an included source line acquire a different concat_reset row from an equivalent root line.

Restricting flattening only in name resolution repaired those failures but was too late for function discovery. A literal conditional declaration in a declare body stayed undefined while the ordinary literal conditional compiled. Normalizing at the parser statement-list boundary repairs that case without claiming arbitrary runtime conditional-function support.

## Watch out
Check arrays, declaration discovery, namespace bodies, lexical strictness and source maps separately. Keep prototype failures distinct from final evidence. A mixed runtime strict-argument probe exposed a separate boundary gap; the runnable example demonstrates the supported static call-site scope instead and matches PHP. Costs were not measured; zero disables historical attribution.
