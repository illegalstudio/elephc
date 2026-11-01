---
id: gotcha-the-reachability-graph-cloned-a-whole-classnode--3dc210de
type: gotcha
title: "The reachability graph cloned a whole ClassNode per implementing class, only to end a borrow"
description: "scan_new_classes cloned every method's Usage twice per scanned class and once per interface per implementer; taking the target set out instead cut graph_compute 20.37s to 15.43s"
created: 2026-09-20
sources:
  - path: src/optimize/reachability/graph.rs
    blob: 61b5305ea05722d9966f51a325a03d380b3d2566
    lines: 377-440
    snip: 977ca077fc74
    anchor: "if self.reach.functions.contains(group) {"
---

# The reachability graph cloned a whole ClassNode per implementing class, only to end a borrow

## Fact

`ClassNode` carries `methods: HashMap<(String, bool), Usage>` — a full `Usage` per method. In
`scan_new_classes` the node was fetched as `.get(&name).cloned()`, so every method's `Usage`
was deep-cloned, and then `node.usage.clone()` cloned the class-level one a SECOND time. Worse,
the interface-contract loop did `self.index.classes.get(interface).cloned()` — the whole
interface node, every method's `Usage` with it — once per implementing class, purely so the
immutable borrow of `self.index` would end before writing `structural_referenced_methods`.

The fix has two halves and both are already idiomatic in this file:

1. Clone only the fields the loop reads (`usage`, `parent`, `interfaces`, `traits`, and the
   method KEYS, and those only for an interface) instead of the node.
2. `std::mem::take(&mut self.structural_referenced_methods)` into a local, write into the
   local while reading `self.index`, put it back. Nothing between the take and the restore
   reads that set, so taking it cannot lose an entry — the same argument the `seed_live_methods`
   comment already makes for `reach.classes`.

Measured: `graph_compute` 20.37 s -> 15.43 s, whole build 126.40 s -> 110.13 s, with
`asm_bytes`/`asm_lines`/`bin_bytes` unchanged to the digit.

**What you must NOT do here**: `std::mem::take(&mut self.index)` to end the borrow wholesale.
`apply_usage` reaches `keep_class_alias_target_methods`, which reads `self.index.classes`
(graph.rs:811). An emptied index there prunes a live declaration and the build still links —
it fails at run time as a missing method. Take the set being WRITTEN, never the index.

## Why

This is the third instance of the same shape in this file, and the fix is always the same: the clone exists to end an immutable borrow, and mem::take on the set being written removes the need for it.
