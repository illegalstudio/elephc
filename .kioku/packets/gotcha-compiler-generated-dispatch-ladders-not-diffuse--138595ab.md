---
id: gotcha-compiler-generated-dispatch-ladders-not-diffuse--138595ab
type: gotcha
title: "Compiler-generated dispatch ladders, not diffuse code, are what makes the Symfony build slow"
description: "Four bodies emit over a million assembly lines each; 87.6% of the assembly comes from the 15.7% of bodies that touch a shared cache"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 415654f395442399d367c4761f36ad100e95ad44
    lines: 120-200
    snip: c07946a8d642
    anchor: "let mut inventory = BackendInventory::from_env();"
---

# Compiler-generated dispatch ladders, not diffuse code, are what makes the Symfony build slow

## Fact

Code generation is 55% of a Symfony `--web` build and emits 61 million assembly lines. That looks
diffuse when lines are attributed to the nearest LABEL -- no symbol family exceeds 6.7% -- and
the attribution is what was wrong. Code generation writes `@fn name=... symbol=...` and
`@endfn` markers around every body, and attributing to those (`scratchpad/asm-per-body.py`) says
something else entirely:

    42,187,840 bytes  1,240,384 lines  ReflectionAttribute::newInstance
    40,244,481 bytes  1,258,814 lines  __eir_closure_..._Console_Attribute_Ask__tryFrom_0
    36,893,857 bytes  1,219,052 lines  ReflectionParameter::getDefaultValue
    34,865,371 bytes  1,096,266 lines  Twig\Extension\CoreExtension::getAttribute
    23,814,868 bytes    829,762 lines  __elephc_is_callable_ext
    top 50 bodies = 42.9% of all body bytes

One PHP method emitting 1.2 million instructions is a dispatch ladder over the whole closed
world -- every attribute, every parameter default, every callable -- lowered inline instead of
through a table. Inside `ReflectionParameter::getDefaultValue` there are 74,891 `movz/movk/sub`
address computations over 12,728 distinct frame slots, so the frame itself is ~200 KB; the
zeroing is not the cost, the ladder is.

This is also what caps the parallel body pass. Bodies that touch a shared cache must be emitted
serially, and `ELEPHC_CODEGEN_SHARED_TOUCHES=1` reports:

    bodies=8786 touching_shared_caches=1383 independent=7403
    parallel_bytes=367,928,617 deferred_bytes=2,605,414,876 deferred_share=87.6%

15.7% of bodies, 87.6% of the assembly -- because the ladder-bearing bodies are exactly the ones
that reach the callable, reflection and eval caches. So parallelising the independent bodies has
a ceiling of 12.4% of code generation, and the measured A/B agrees (188 s -> 158 s under load).

Making those ladders table-driven is the order-of-magnitude lever: it would shrink code
generation, assembling, the binary and the `.s` at once. Parallelising the REST needs the shared
caches made concurrent, which is cheap in lock terms now that labels no longer come from a shared
counter (about 10k accesses, not 3.6M) but needs helper bodies emitted into keyed buffers and
sorted at merge, or the output stops being byte-identical between runs.

## Why

Parallelising code generation can only reach the other 12.4%; the ladders are the lever worth an order of magnitude
