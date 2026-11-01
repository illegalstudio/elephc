---
id: bug_fix-compiler-output-depended-on-hash-map-order-in-th-de9bd3c8
type: bug_fix
title: "Compiler output depended on hash-map order in three emitters; a same-process double lowering catches the next one"
description: "Loop storage contracts, extern_decls and the source-activate table emitted in HashMap order, so builds differed and the asm slice cache missed"
created: 2026-09-27
sources:
  - path: src/ir_lower/stmt/conditionals.rs
    blob: 9e6380498a84e30af3769c51e9a91818c40e6433
    lines: 458-472
    snip: cd691701fed7
    anchor: "let mut contracts: Vec<(String, PhpType)> = ctx"
  - path: src/ir_lower/program/metadata.rs
    blob: cdf24743ea8597718eca473ebbe83d05ba972c3a
    lines: 53-75
    snip: 1ad4cb5f5e01
    anchor: "module.extern_decls = check_result"
---

# Compiler output depended on hash-map order in three emitters; a same-process double lowering catches the next one

## Fact

Two builds of the same Symfony program produced different assembly (~10 KB of 1.49 GB), because
three places EMITTED in a std HashMap's iteration order, and std seeds every map separately (per
process AND per map):

- apply_loop_storage_contracts (ir_lower/stmt/conditionals.rs) converted loop-carried locals in
  TypeEnv order, so the boxing sequence at a loop edge changed from run to run;
- module.extern_decls was collected from check_result.extern_functions.values(), which reordered
  the function_exists() candidate table;
- emit_source_activate iterated module.declared_class_source_files.

Each now sorts where it emits. ir_lower::tests::lowering_is_deterministic_across_runs lowers one
program four times in one process and compares the printed EIR; it fails without the sort.

Why it matters beyond tidiness: the assembler's per-slice object cache only hits on identical
slices, so a nondeterministic compiler re-assembled slices it had already built. After the fix,
two Symfony builds are byte-identical.

To find the next one: compile a small file ten times with `--emit-ir --ir-opt=off` and count the
distinct outputs; if they differ, diff two of them, and the span in the diff names the construct.
For a whole application, build twice under the SAME entry path (it is embedded in the output) and
compare function by function.

## Why

Deterministic output is what lets the per-slice assembler cache hit; any new emitter that walks a std map reintroduces the drift
