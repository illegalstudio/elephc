---
id: decision-symfony-web-compile-140s-to-85s-one-shape-explai-14c75bff
type: decision
title: "Symfony --web compile 140s to 85s: one shape explains every win, and four hypotheses the measurements killed"
description: "Five memoized module scans, an inverted cross product, and an instruction borrowed instead of cloned; CPU 132.48s to 78.57s with a byte-identical .s at every step"
created: 2026-09-20
sources:
  - path: src/codegen/lower_inst.rs
    blob: 5639821a127d267b5abaaf3b92557b83eb68f645
    lines: 155-172
    snip: 7326e8a26103
    anchor: "pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {"
  - path: src/optimize/reachability/graph.rs
    blob: 8615043211f25841290de1ee8bacef6bde8f1903
    lines: 700-780
    snip: 13f7ce2c593e
    anchor: "fn seed_inherited_implementations(&mut self) {"
  - path: src/types/checker/method_pass.rs
    blob: a93af5fda16303dd4f41c0a990745c1c17b1c4d2
    lines: 42-120
    snip: e648dba0a620
    anchor: "pub(super) fn type_check_methods_until_stable("
---

# Symfony --web compile 140s to 85s: one shape explains every win, and four hypotheses the measurements killed

## Fact

Every step below kept `asm_bytes=1396032370`, `asm_lines=42114361`, `bin_bytes=178246552`
unchanged, which is the proof that only recomputation was removed. The check is free: the
assembler slice cache is content-addressed, so a `.s` that moved misses every slice and the
assemble phase triples.

    step                                     phases     user CPU
    start                                    140.10s    132.48s
    per-candidate folds + dispatch memo      126.40s    113.78s
    reachability cross product inverted      113.59s     91.03s
    EIR instruction borrowed, not cloned      85.30s     78.57s

Per phase, `graph_compute` went 17.29s -> 3.01s and declaration pruning ~22.5s -> ~4s.

ONE SHAPE explains every win: the compiler copied or re-scanned data to end a borrow that did
not need ending.

  * five whole-module predicates asked once per body      -> memoized in SharedCodegenState
  * `seed_inherited_implementations`, a cross product of
    live classes x referenced method names                -> driven by what the chain declares
  * a whole `ClassNode` cloned per implementing class     -> clone the fields the loop reads
  * an `Instruction` cloned per instruction of every body -> copy the `&'a Function` first

The last one is the cheapest and the biggest. `FunctionContext::function` is a `&'a Function`
whose lifetime is the struct's parameter, NOT the borrow of `ctx`, so

    let function = ctx.function;
    let inst = function.instruction(inst_id).ok_or_else(...)?;

hands every `lower_*` arm a borrow that outlives the `&mut ctx` they take. 221 arms then pass
`inst` instead of `&inst` and it compiles unchanged.

FOUR HYPOTHESES THIS METHOD KILLED, so nobody spends a day on them again:

  1. the `--web` small-bin double-free guard (est. 13% of a request; measured: nil, on/off/on
     with the off reading inside the spread between the two on readings);
  2. the heap free list's two O(n) walks per free (66 nodes, walked from lldb);
  3. `expand_function_variants` in the reachability fixpoint (0.00s);
  4. the full class-table clone in the type-check fixpoint (0.39s of 32.61s) -- and this one
     would have been WORSE than useless: the field that oscillates is not among the three a
     pass writes, so a narrowed snapshot would have declared stability a pass early.

The board of advisor models was right about the SHAPE and wrong about the STATE three times:
its top request-side item (hoisting the eval native-surface registration out of the request)
was already implemented, and so were two others. Feed a board the current measurements, not
the repo's memory of them.

## Why

The wins and the dead ends come from the same discipline: instrument the phase, read the profile, and never optimise a shape that looks expensive without measuring it.
