---
id: runbook-consulting-kimi-glm-deepseek-and-gpt-5-6-sol-on--1dcb6186
type: runbook
title: "Consulting Kimi, GLM, DeepSeek and GPT 5.6-Sol on an elephc design question"
description: "Round 1 always burns its tool budget exploring; feed the verified findings back and demand a ranked plan in round 2"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 7e6373865f9569698c31b12f913636c882dd63eb
    lines: 120-160
    snip: d8db958bbfad
    anchor: "let mut inventory = BackendInventory::from_env();"
---

# Consulting Kimi, GLM, DeepSeek and GPT 5.6-Sol on an elephc design question

## Fact

The user asks for design and optimisation questions to be put to Kimi K3, GLM 5.3 and DeepSeek
(Ollama cloud models, `POST localhost:11434/api/chat` with a `tools` array) plus GPT 5.6-Sol at
`model_reasoning_effort=xhigh` through `codex exec -m gpt-5.6-sol -s read-only -C <worktree>`.

**Give them real code access.** `scratchpad/optimize_agent.py` is a tool loop exposing
`list_dir`, `read_file` (with a line range) and `grep`, every path resolved through a guard that
refuses anything outside the worktree. Without it a model cannot navigate elephc and produces
nothing usable.

**Two rounds, not one.** With a 40-step budget all three Ollama models spent every step
exploring and never wrote a plan. Round 2 is the one that pays: hand back the round-1 findings
you VERIFIED, mark them established, and say "a ranked plan, not more exploration; spend at most
half your tool budget checking code you need to name". Codex produced its plan in one pass.

**Ask for the failure mode.** Requiring "how could this silently break, and the cheapest test
that catches it" is what made the answers usable — each proposal came with the existing test
file that covers it.

**Verify everything.** Of the four headline claims:
- "the parallel-codegen ceiling is an artifact of counter-derived helper labels" — CONFIRMED by
  all four independently and by a new measurement (worker pass 43.55 s, serial tail 45.03 s).
- "`print_r`-style per-class string ladders: `emit_branch_if_dynamic_new_mixed_class_name_matches`
  emits ~10 instructions and a `__rt_strcasecmp` PER CANDIDATE CLASS" (GPT) — CONFIRMED in the
  source; it is the 797,768 `strcasecmp` calls.
- "the per-function `<fn>_epilogue` regions are dead" (GLM) — CONFIRMED (8,953 defined, 0
  referenced) but only 0.13% of lines, so not a lever. Verifying the MAGNITUDE mattered as much
  as verifying the claim.
- "cap the inliner" (GPT's top pick) — FALSIFIED by measurement: `ELEPHC_INLINE_BUDGET=50` moved
  the emitted assembly by 0.4%.
And one was simply wrong: DeepSeek's scaled load/store immediate does not apply, because frame
slots sit at NEGATIVE offsets from `x29` where AArch64 only has the signed 9-bit `stur`/`ldur`.

Convergence between two models on the same file and line is the strongest signal; a single
model's confident claim is a lead, not a fact.

## Why

Four models converged on the right lever and two of them found things three sessions had missed
