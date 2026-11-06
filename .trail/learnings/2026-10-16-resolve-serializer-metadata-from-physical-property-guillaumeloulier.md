---
title: Resolve serializer metadata from physical property slots
author: Guillaumeloulier
date: 2026-10-16
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen_support/runtime/data/user.rs
  - tests/codegen/serialize/private_slot_shadowing.rs
---

## Decided

Resolve serializer metadata per physical property index through the nearest ancestor where that slot remains visible. Use the resolved owner consistently for PHP-mangled keys, offsets, storage tags, declaring-class IDs and type diagnostics. Reuse physical_property_slot_metadata without applying the renderer's debugInfo projection.

## Tried and ruled out

The frozen published compiler rejected a valid child public string property with the private parent's int mask. Name-keyed maps target the child even for the parent's layout row. Changing a diagnostic alone leaves keys and storage tags inconsistent. The debugInfo projection deliberately omits or renames rendered properties and cannot serve as a serialization layout.

## Watch out

Check complete objects and wire bytes, including both NUL-mangled parent and public child keys, with IR optimization on/off. Include a private string parent plus scalar child so heap-bearing storage cannot masquerade as a scalar. Successful typed hydration already retains parsed cells on distinct-name slots (2 blocks/88 bytes in the control); compare retained accounting rather than claiming zero leaks. Rejected-value cleanup remains a separate clean-heap gate. An inferred integer default on an untyped property has a separate pre-existing storage-shape limitation; do not hide it behind acceptance-only output.
