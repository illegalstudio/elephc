---
title: "SimpleXML after the main merge: the branch's generic lowerings sit in front of main's native XML paths"
author: Guillaume
date: 2026-11-21
branch: bead-simplexml
touches:
  - src/ir_lower/expr/lazy_branches.rs
  - src/ir_lower/expr/lazy_isset.rs
  - src/ir_lower/expr/numeric_binary.rs
  - src/ir_lower/expr/array_access.rs
  - src/ir_lower/expr/ref_place_args.rs
  - src/ir_lower/expr/property_access.rs
---

## Found
- The simplexml tests never passed before the main merge: the pre-merge branch (30779c105b) has no SimpleXML at all. They fail because the branch rewrote the generic lowerings that main's SimpleXML paths sit behind, and the merge kept the branch's version in front.
- Pattern: a branch lowering answers for an "opaque" receiver (mixed, union, object, interface) or a boxed object before main's native XML dispatch gets a chance. Fixes put the native path back in front: `??` (lazy_branches.rs, native probe with the Coalesce mode for non-XML), `empty()` on opaque receivers (lazy_isset.rs: runtime-name probe, __isset alone), isset/empty probes (`property_can_be_uninitialized_for_presence`), bitwise on a mixed XML operand (numeric_binary.rs: the gradual string/int dispatch read the object as 0), by-ref dynamic property args (ref_place_args.rs `static_place_type`), `$xml[k]` on an XML subclass (array_access.rs: `receiver_is_unproven_array_access` boxed it and leaked the box on a throw).
- `lower_property_probe_without_native` ignored its `mode` argument after the merge (always Probe), so isset() ran __get.
- Eval: `eval_xml_bridge_linked` keys on `xml_parser_create`; a SimpleXML-only program does not have it, so `simplexml_load_string()` in eval was "undefined". Check the function asked for.
- eval `var_export` (branch-only file) had no native XML branch; var_dump/print_r do.

## Tried and ruled out
- Filtering interfaces out of the runtime-class arm loops (native_property_children, runtime_receivers, magic_index) and out of reserve_property_hash_storage does not clear "unknown dynamic-property class <Interface>": the arms name the right classes but their unboxed value prints as the interface type (bd issue filed).
- Reproducing the bitnot heap leak from the CLI: clean there, leaks only in the test harness (8 MB heap, library entry).
