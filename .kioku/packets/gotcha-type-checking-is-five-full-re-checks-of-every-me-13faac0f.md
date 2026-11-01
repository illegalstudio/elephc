---
id: gotcha-type-checking-is-five-full-re-checks-of-every-me-13faac0f
type: gotcha
title: "Type checking is five full re-checks of every method body, and the last two chase one oscillating signature"
description: "methods_until_stable is 98% of the typecheck phase, but the clone and the deep comparisons are 0.57s of it: the cost is bodies=29.26s over 5 passes that end on cycle detection, not convergence"
created: 2026-09-20
sources:
  - path: src/types/checker/method_pass.rs
    blob: 301f36d18f7b237dbf24966fa0d47e6db3be2c6d
    lines: 42-330
    snip: 7ffcb93ecfac
    anchor: "pub(super) fn type_check_methods_until_stable("
---

# Type checking is five full re-checks of every method body, and the last two chase one oscillating signature

## Fact

`ELEPHC_TYPECHECK_TIMES=1` now splits both levels:

    [elephc-typecheck]  declarations=0.60s  top_level_first=0.01s  unchecked_functions=0.05s
                        methods_until_stable=32.61s  top_level_final=0.00s  total=33.30s
    [elephc-methodpass] passes=5 clone=0.39s bodies=29.26s compare=0.18s
                        classes=1025 stabilized=false cycling=true

So the type-checking phase IS `type_check_methods_until_stable` (98%), but inside it:

* the full `self.classes.clone()` per pass -- dozens of maps per `ClassInfo`, including every
  class constant's AST -- costs **0.39s total**, not the tens of seconds its shape suggests;
* the stability comparison plus up to four cycle-window comparisons cost **0.18s total**;
* **re-checking every method body costs 29.26s**, five times over.

A snapshot narrowed to the three fields a pass actually writes (`methods`, `static_methods`,
`callable_method_return_sigs`) would therefore buy about half a second. Do not bother.

The two levers the numbers do point at:

1. **The loop never converges.** It exits on `cycling=true`, which means passes 4 and 5
   re-checked all 1025 classes to chase the single oscillating signature the existing comment
   already describes ("quiet for every class but one after three passes and then oscillates
   forever on a single property"). Ending sooner, or isolating the oscillating symbol and
   re-checking only it and its dependents, is worth roughly 12s.
2. **Nothing is incremental.** Every pass re-checks every body regardless of whether any input
   to that body moved. A worklist keyed on "signatures this body reads" would collapse passes
   2-5 to a fraction. This is the bigger and harder one.

## Why

The obvious fix -- stop cloning the whole ClassInfo per pass -- is worth half a second; measuring first is what showed the loop is re-checking bodies, not copying tables.
