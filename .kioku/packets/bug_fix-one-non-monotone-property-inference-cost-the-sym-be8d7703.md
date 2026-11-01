---
id: bug_fix-one-non-monotone-property-inference-cost-the-sym-be8d7703
type: bug_fix
title: "One non-monotone property inference cost the Symfony build 54 minutes of type-check passes"
description: "The method-pass fixpoint ran its whole 1630-pass budget because Twig Parser embeddedTemplates alternated between two types; joining inferred property types and stopping on a repeated class table took the preload from 54 minutes to 14 seconds"
created: 2026-09-17
sources:
  - path: src/types/checker/method_pass.rs
    blob: 056ac5ffd2aff9e9025c8d9833f0852618e01bdd
  - path: src/types/checker/stmt_check/assignments/properties.rs
    blob: 8df0bd454412dc432d833fc42ecda2e548d2b744
---

# One non-monotone property inference cost the Symfony build 54 minutes of type-check passes

## Fact

One non-monotone property inference made the Symfony `--web` build take 54 minutes instead of 14
seconds.

`type_check_methods_until_stable` re-checks every method body until the class table stops changing,
with a budget of `classes * 2 + 1` decrements — about 1630 passes for 814 classes. Measured on the
246-file preload:

| pass | classes that changed |
|---|---|
| 0 | 568 |
| 1 | 41 |
| 2 | 4 |
| 3 and on | 1, forever |

The one is `Twig\Parser`, whose `$embeddedTemplates` alternates between `array<Twig\Node\ModuleNode>`
and `array<mixed>` on every pass: `$this->embeddedTemplates[] = $template` re-narrows what a read
into a bare `array` parameter had widened. A fixpoint may only ever widen, so the loop could not
settle and ran the whole budget at ~2.0s a pass. The 165-file preload settles in 4 passes and 8s,
which is why the cost read as superlinear in the file count rather than as a runaway.

Two changes, both needed:

1. `update_object_property_type` JOINS an inferred property's stored type instead of overwriting it
   (`merged_assignment_type`, `Mixed` when there is no common type). A declared property is
   untouched. This is also the honest answer — the slot has to hold every value any site assigns.
   With it, the checker's output on the preload is byte-identical to the 1630-pass run.
2. The loop stops when the class table REPEATS one it already produced, not only when it settles. A
   repeat adds nothing and never will: the next table is a function of that one alone, so the
   sequence is periodic from there. `PASS_CYCLE_WINDOW` remembers four tables; the clone is not
   extra, the loop already made one per pass to answer `stabilized`.

Measured: `--check` on the 246-file preload 54min → 14s, `--web` likewise; the 165-file preload is
unchanged at ~8s because it already converged. `error_tests` keeps the exact clean-HEAD failure set
(99, same names) and `codegen::arrays` + `codegen::regressions` keep theirs (33, same names).

Monotonicity alone does NOT make the preload converge — something else still oscillates — so the
repeat check is load-bearing, not belt and braces. Finding the next one is the same recipe: count
the classes that differ per pass, and when the count settles at one, print which field of that
`ClassInfo` moves.
