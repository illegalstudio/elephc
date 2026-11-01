---
id: gotcha-the-twig-parser-property-oscillation-is-bounded--1e7de16f
type: gotcha
title: "The Twig Parser property oscillation is bounded but not gone: it still costs two of five method passes"
description: "embeddedTemplates still alternates Array(Mixed) and Array(Object(ModuleNode)); cycle detection caps it at 5 passes instead of 1630, and passes 4-5 re-check all 1025 classes to chase it"
created: 2026-09-20
sources:
  - path: src/types/checker/stmt_check/assignments/properties.rs
    blob: 8df0bd454412dc432d833fc42ecda2e548d2b744
    lines: 619-700
    snip: 73aa61391ab2
    anchor: "pub(super) fn refined_untyped_property_assignment_type("
  - path: src/types/checker/method_pass.rs
    blob: a93af5fda16303dd4f41c0a990745c1c17b1c4d2
    lines: 42-120
    snip: e648dba0a620
    anchor: "pub(super) fn type_check_methods_until_stable("
---

# The Twig Parser property oscillation is bounded but not gone: it still costs two of five method passes

## Fact

Measured with `ELEPHC_TYPECHECK_TIMES=1` on the Symfony `--web` build:

    pass=1 moved=1136
    pass=2 moved=97
    pass=3 moved=4    Composer\Autoload\ClassLoader::getclassmap, StreamOutput::getstream,
                      Twig\NodeVisitor\EscaperNodeVisitor, Twig\Parser
    pass=4 moved=1    Twig\Parser: embeddedTemplates Array(Object("Twig\Node\ModuleNode")) -> Array(Mixed)
    pass=5 moved=1    Twig\Parser: embeddedTemplates Array(Mixed) -> Array(Object("Twig\Node\ModuleNode"))
    passes=5 bodies=16.04s stabilized=false cycling=true

So the loop still exits on CYCLE detection, never on convergence, and the last two passes exist
only to chase one property. Each pass re-checks every method body of all 1025 classes, so those
two passes are roughly 40% of the 16s the bodies cost.

The property is `private $embeddedTemplates = [];` in `twig/src/Parser.php` -- UNTYPED, so the
join at properties.rs:1188 ("an INFERRED property's stored type may only widen") should apply
and make it monotone. It does not, and the two sites are the plain assignment
`$this->embeddedTemplates = []` (line 104) and the APPEND `$this->embeddedTemplates[] =
$template` (line 328). The append reaches the property type through a different path than the
plain assignment, and that path is where the widening is missing: once the slot has reached
`Array(Mixed)` nothing should narrow it back to `Array(Object(...))`.

Note also that the oscillating field is NOT one of `methods` / `static_methods` /
`callable_method_return_sigs` -- the three a method pass writes in `method_pass.rs`. So a
"snapshot only the fields a pass writes" optimisation of the fixpoint would MISS this and
declare stability one pass early. Measuring the loop first is what caught that; the clone it
would have removed costs 0.39s of 32.61s and is not worth touching at all.

## Why

The earlier fix is recorded as solving this; it bounded the cost rather than removing it, and the exact type pair says which rule is still non-monotone.
