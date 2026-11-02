---
id: decision-assembly-label-ids-are-scoped-to-the-emitting-bo-f0832a34
type: decision
title: "Assembly label ids are scoped to the emitting body, not to a module-wide counter"
description: "Removes the shared counter from 1911 codegen call sites, the last obstacle to emitting bodies in parallel, and shrinks the emitted assembly 8.2%"
created: 2026-09-20
sources:
  - path: src/codegen/context.rs
    blob: 36e0fdaabdc869ee0aed02f2f408290ff3ba6f56
    lines: 40-70
    snip: 8ed608253433
    anchor: "const LABEL_FRAGMENT_BUDGET: usize = 24;"
  - path: src/codegen/context.rs
    blob: 36e0fdaabdc869ee0aed02f2f408290ff3ba6f56
    lines: 190-226
    snip: 32d48034b6e4
    anchor: "pub(super) fn next_label(&mut self, prefix: &str) -> String {"
---

# Assembly label ids are scoped to the emitting body, not to a module-wide counter

## Fact

Code generation is 99.5 s of a 180 s Symfony `--web` build and runs on one core. The state each
body shares with the others is what stands in the way of emitting bodies in parallel, and 1911 of
the roughly 1930 touch sites were `next_label()` reaching into `SharedCodegenState` for the next
module-wide id. A mutex on 3.6 million label allocations would serialise the work; an atomic
counter would make the output depend on thread interleaving, which
`tests/compiler_determinism_tests.rs` forbids.

The id is now `<48-bit FNV-1a of the body name>_<counter private to the FunctionContext>`. Both
halves are fixed before any thread starts. Every function reaches exactly one `FunctionContext`
(a generator through `emit_generator_body`, a plain body through `emit_user_function` or
`emit_class_method`, a synthetic helper with a name of its own), so two contexts never share a
name; if that ever stops holding, the assembler rejects the duplicate label rather than emitting
a silent miscompile.

FNV-1a and not a standard-library hasher: the value is written into the assembly, so it has to be
identical on every run and host, which `RandomState` is not.

With uniqueness carried by that pair, the readable part of a label no longer has to be complete,
and it was enormous -- labels averaged 97.6 characters because each spelled out a whole
fully-qualified PHP name. Capping it at 24 characters took the emitted assembly from 2.23 GB to
2.05 GB (-8.2%) on the Symfony app, with the same line and label counts.

Verified neutral: determinism tests 3/3; `codegen_tests arrays` 914/30, `print_r` 47/0,
`var_dump` 36/0, `var_export` 7/0, `print_r_object` 13/0, `print_r_return_mode_heap` 6/1,
`var_dump_nested` 13/0, `var_dump_object` 54/0, `var_export_and_strstr_result` 26/3 -- every
count identical to before the change; and all eight Symfony routes still byte-identical to
`php -S`.

What is still needed for parallel emission: a per-shard `DataSection` with a namespaced label
prefix plus a merge that dedups `comm`, named symbols and static locals by name; and shared
helper bodies (callable invokers, builtin/extern wrappers, instance-method descriptors, eval
registration, reflection materializers -- about eight kinds) emitted into keyed buffers that are
sorted at merge, so their placement does not depend on which shard reached them first.

## Why

Code generation is 55% of a Symfony --web build and single-threaded; a mutex on 3.6M label allocations would serialise exactly what parallelism is meant to spread
