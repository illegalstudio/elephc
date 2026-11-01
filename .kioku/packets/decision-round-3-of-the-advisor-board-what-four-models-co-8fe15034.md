---
id: decision-round-3-of-the-advisor-board-what-four-models-co-8fe15034
type: decision
title: "Round 3 of the advisor board: what four models converged on for the last 20 seconds"
description: "Kimi, GLM, DeepSeek and GPT 5.6-Sol ranked the remaining compile-time work independently; the overlap table and which claims survived checking the source"
created: 2026-09-20
sources:
  - path: src/codegen/lower_inst/objects/reflection/constant_metadata.rs
    blob: cced7ba317540af605f15fb5e675e16463c63005
    lines: 386-440
    snip: 93a968c8979c
    anchor: "pub(super) fn resolve_reflection_class<'a>("
  - path: src/ir_lower/expr/method_metadata.rs
    blob: 72bd8e062128d79db8caf6c5e2070b8910f2a689
    lines: 125-200
    snip: fe45a0791f29
    anchor: "pub(super) fn method_return_arg_alias("
---

# Round 3 of the advisor board: what four models converged on for the last 20 seconds

## Fact

Brief: the 140 s phase breakdown, the five already-fixed whole-module scans, the falsified
levers (emitted volume, parallel codegen) and the unfixed sampler names. Each model got
read-only tools and was told to spend at most half its budget checking code.

Overlap (4 models, 6 items each):

  resolvers in constant_metadata.rs -> the shared key index   4/4   <- applied
  a hot map still on SipHash                                  4/4   (2/4 named shared_reflection.rs, applied)
  the whole-class-table scans in EIR lowering                 3/4
  asm_split materialising 18 owned Strings from 1.4 GB        3/4
  the reachability fixed point                                3/4   (GPT 5.6-Sol calls it the only threshold-crosser)
  canonical_builtin_reflection_class_name                     3/4

What checking the source changed:

- DeepSeek's two headline claims were both exactly right, including the file and line of the
  SipHash. Its stated failure mode for the hasher swap ("if anything iterates `materializers`
  to emit labels") does not apply: that map is only get/insert/remove.
- GLM was wrong that shared_state.rs still held the SipHash — it had already been converted —
  but right that other std-hashed maps remain.
- Kimi and GLM both feared the EIR memo would be unsound because metadata might be mutated
  during lowering. It cannot be: `LoweringContext` holds `classes`, `interfaces` and
  `return_alias_summaries` as `&'m` SHARED borrows, so the borrow checker forbids mutation for
  the whole phase. The type answers the objection; no audit was needed.
- `ReturnArgAlias::merge` absorbs on Unknown, is neutral on None and unions elsewhere, so it is
  commutative and associative — which is what makes memoizing a fold over a hash map's
  arbitrary iteration order legitimate.

Ranking by seconds-over-risk was consistent; where the models differed was only on whether
120 s is reachable without restructuring the reachability fixed point. GPT 5.6-Sol says no,
Kimi says yes if the EIR item lands at the top of its range.

## Why

Convergence between models on the same file and line is the signal worth acting on; a single confident model is a lead.
