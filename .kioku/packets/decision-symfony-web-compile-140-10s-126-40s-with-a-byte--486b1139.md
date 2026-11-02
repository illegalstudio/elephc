---
id: decision-symfony-web-compile-140-10s-126-40s-with-a-byte--486b1139
type: decision
title: "Symfony --web compile: 140.10s -> 126.40s with a byte-identical .s, and how the assembler cache proves it"
description: "The per-candidate lowercase allocations and one memoized dispatch fold took 13.7s wall and 18.7s CPU off the build; the slice cache is a free byte-identity oracle"
created: 2026-09-20
sources:
  - path: src/ir_lower/expr/method_metadata.rs
    blob: 72bd8e062128d79db8caf6c5e2070b8910f2a689
    lines: 120-200
    snip: f45c412c0f30
    anchor: "pub(super) fn method_return_arg_alias("
  - path: src/types/return_alias.rs
    blob: 638cebe013d7724a8123ae384a98167b7ccebba1
    lines: 63-100
    snip: 42e0da4e0c55
    anchor: "pub(crate) struct ReturnAliasSummaries {"
  - path: src/linker/mod.rs
    blob: a63a70e5b0a1c3cb8011afad8fa1be5ce4d032f7
    lines: 163-225
    snip: 192e9e8831d0
    anchor: "pub(crate) fn assemble_parallel("
---

# Symfony --web compile: 140.10s -> 126.40s with a byte-identical .s, and how the assembler cache proves it

## Fact

Changes in this batch, all in the two phases they target:

  codegen   the four reflection resolvers in constant_metadata.rs route through the shared PHP
            key index instead of scanning class_infos/interface_infos/trait_table/enum_infos
  codegen   shared_reflection.rs moved off the std hasher (FastMap/FastSet)
  lowering  eight `php_symbol_key(a) == php_symbol_key(b)` scans became `eq_ignore_ascii_case`
  lowering  `method_return_arg_alias` memoizes its two whole-class-table folds in
            `ReturnAliasSummaries` (new `DispatchFoldKey`), sound because `LoweringContext`
            holds `classes` and the summaries as `&'m` SHARED borrows: the borrow checker
            forbids mutation for the whole phase
  lowering  `common_dynamic_method_signature` compares borrowed `FunctionSig`s and clones only
            the winner, instead of cloning one per class

Measured, same machine, load 25.58 -> 21.71:

                                  before     after
  Checking types                  22.65      25.26    (load)
  Pruning unreachable decls       22.52      21.95
  Lowering program to EIR         13.81      11.18
  Generating native code          51.24      40.92
  Assembling object file          22.97      19.74
  TOTAL                          140.10     126.40
  user CPU                       132.48     113.78

**The assembler slice cache is a free byte-identity oracle.** `assemble_parallel` keys each
slice by an FNV hash of its bytes (src/linker/asm_cache.rs) and restores a hit as a hardlink,
running no assembler at all. So a run whose `.s` is byte-identical to an earlier one shows an
assemble phase of ~20 s (read + split + hash only) instead of ~40-100 s, and `asm_bytes` /
`asm_lines` / `bin_bytes` come back to the digit. Here all three matched the baseline exactly:
1396032370 / 42114361 / 178246552. No separate diff run is needed.

Trap this caught: `build-symfony.sh` defaulted to `jobs=8`. At 8 the workers defer every body
that touches a shared cache and re-emit 153 397 MORE lines, so `asm_bytes` moves, every slice
misses the cache, and the assemble phase triples. A run at the old default is not comparable
to any sfbuild-*.log baseline. The default is now 1.

## Why

Every previous compile-time claim in this campaign was validated by a byte-identical .s; this records the cheap way to check that, and the numbers for the batch.
