---
id: decision-the-compile-time-plan-four-models-converged-on-w-f6d61856
type: decision
title: "The compile-time plan four models converged on, with the measurement that settles it"
description: "Serial tail 45.03s vs worker pass 43.55s: the parallel-codegen 12.4% ceiling is an artifact of deferral, not a dependency limit"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: bbad3e5d0a4b4e664c61f2e05522cce5740b390d
    lines: 160-215
    snip: 94453ad406c4
    anchor: "let jobs = codegen_jobs();"
  - path: src/codegen/lower_inst/objects/dynamic_mixed_candidates.rs
    blob: ac304c0495f480c12f4a4118fd1a7523145840fe
    lines: 418-445
    snip: e697ad91db40
    anchor: "pub(super) fn emit_branch_if_dynamic_new_mixed_class_name_matches("
---

# The compile-time plan four models converged on, with the measurement that settles it

## Fact

Kimi K3, GLM 5.3, DeepSeek v4.1 (via Ollama with read-only repo tools) and GPT 5.6-Sol at xhigh
(via Codex) were each given the measured brief and the repository. They converged, and the
convergence was checked against the source and against a new measurement.

**The measurement that settles it** (`ELEPHC_CODEGEN_SHARED_TOUCHES=1` now reports both):

    worker_pass=43.55s          all 8786 bodies, 8 threads, deferred output thrown away
    serial_tail=45.03s          the 1383 cache-touching bodies, re-emitted on one thread
    Generating native code = 126.16 s

The worker pass emits EVERY body — including the giant ones — in 43.55 s. The serial tail then
re-does 1383 of them in 45.03 s alone. So removing the deferral is worth roughly 45 s of a 126 s
codegen, and all four models independently said the 12.4% ceiling is an artifact of the rollback
design rather than a real dependency limit.

**Ranked plan**

1. **Freeze shared codegen artifacts so no body defers** (~45 s). Shared-helper labels are minted
   from the EMITTING body (`next_global_label` embeds the body's name and counter), so two
   workers produce two different labels and two copies — which is the only reason a cache touch
   forces serial re-emission. Derive each helper's label AND its internal label scope from its
   semantic cache key (the `body_label_scope` FNV pattern already in `context.rs` exists for
   exactly this), then dedup at merge by key, keeping the lowest worker's copy. Keys are already
   semantic: `ReflectionMaterializerKey`, wrapper `(name, FunctionSig, strict_php)`, invoker
   `(sig, captures)`. Mint sites: `lower_inst/runtime_wrappers.rs`, `lower_inst/externs.rs`,
   `lower_inst/objects/reflection/materializer.rs`, `lower_inst/builtins/eval/context_registration.rs`,
   `lower_inst/callables.rs`. The danger is a cache key missing a discriminator — two different
   helpers collapsing to one label is silent, so pair the `JOBS=1` vs `JOBS=8` byte-identity
   check with the reflection and callable suites.
2. **Replace the dynamic-new string ladder with class-id table dispatch** (GPT's find, verified).
   `emit_branch_if_dynamic_new_mixed_class_name_matches`
   (`lower_inst/objects/dynamic_mixed_candidates.rs:418`) emits, PER CANDIDATE CLASS, two slot
   loads, a symbol address, an immediate and a `bl __rt_strcasecmp` plus `cmp`/`b.eq` — about ten
   instructions, once per candidate at every dynamic-new site. That is the 797,768 `__rt_strcasecmp`
   calls (1.30% of lines, several times that with their argument setup) and it is what makes
   `ReflectionAttribute::newInstance` and `ReflectionParameter::getDefaultValue` million-line
   bodies. The better primitive already exists: `emit_dynamic_new_class_lookup`
   (`objects/dynamic_factory.rs`) resolves a name to a class id once, and
   `codegen_support/callable_lookup.rs` is the deterministic table to copy.
3. **Overlap code generation with assembling** (Kimi). Seal body-index ranges into their own `.s`
   as workers finish and drain them with the existing `as` pool, instead of waiting for the whole
   2 GB file and then paying a `read_to_string` plus a slice pass.
4. **Share the return epilogue instead of inlining it at every `Return`**. Distinct from the dead
   `<fn>_epilogue` regions (0.13%, already ruled out): the cost is the live inline copy at each
   return. The shared epilogue mechanism is already half-built and currently emitted dead.
5. **Slot coloring / no stack home for register-resident values** in `value_placement::allocate`,
   which today gives every non-void SSA value a unique monotonic slot.

**Falsified, so nobody repeats them**: GPT's top pick was a structural inliner cap — measured,
`ELEPHC_INLINE_BUDGET=50` moved the emitted assembly from 2,047,950,333 to 2,055,972,223 bytes,
i.e. nothing. DeepSeek's scaled-immediate idea does not apply: frame slots are at NEGATIVE
offsets from `x29`, where AArch64 only has the signed 9-bit `stur`/`ldur` form, so reaching past
255 needs a frame-base register, not an encoding change.

**All four agree** extending register allocation to heap/string values (`regalloc.rs::is_eligible`
admits only `I64`/`F64` with `Ownership::NonHeap`) is weeks of work against the highest
miscompile surface in the compiler, and is NOT needed for the 2-minute target.

Harness: `scratchpad/optimize_agent.py` (Ollama tool loop, read-only, scoped to the worktree),
`scratchpad/optimize-brief.md` and `-round2.md`. Round 2 mattered: round 1 spent its whole tool
budget exploring and produced no plan, so feed the verified round-1 findings back and say
"ranked plan, not more exploration".

## Why

Three sessions have guessed at this; the ranked plan below is measured and independently converged
