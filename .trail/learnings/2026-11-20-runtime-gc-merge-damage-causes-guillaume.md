---
title: runtime_gc merge damage causes
author: Guillaume
date: 2026-11-20
branch: bead-gc
cost: { minutes: 478, tokens: 3644893 }
touches:
  - src/ir_lower/stmt/property_array_writes/native_xml.rs
  - src/ir_lower/expr/closures.rs
  - src/codegen/frame.rs
---

## Decided
Three merge-damage causes behind the runtime_gc family were fixed on main design: native_xml ordinary_append skips the DynamicPropSet write-back for object values; initialize_uninitialized_array_property_for_write skips a null-sentinel receiver; closures.rs reads capture_made_cell before promotion, and frame.rs ends the CLI request whenever the eval bridge is linked.

## Tried and ruled out
The 4 leaked blocks at offsets 0..144 are request-retained eval contexts holding the global scope (argv/argc), not argv construction and not the mixed ref-arg write-back.

## Watch out
Build main (git archive bc5b97bfd4) into its own CARGO_TARGET_DIR and diff --emit-ir of a 10-line probe against HEAD: it localised every cause in minutes. Open: foreach by-ref over array<mixed> &$p after eval leaks a null Mixed; ArrayObject nested += loses the write; native reference validator accepts a wrong class.
