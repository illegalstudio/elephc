---
id: decision-instanceof-narrows-to-the-closed-world-s-interse-22b67369
type: decision
title: "instanceof narrows to the closed world's intersection, and private methods resolve in the lexical scope"
description: "A guard the value's own class cannot satisfy names the descendants that satisfy both; private methods reached through such a receiver resolve in the declaring scope in checker, IR lowering and codegen"
created: 2026-09-22
sources:
  - path: src/types/instanceof_intersection.rs
    blob: 2f23f9af3f26b4961e6884b8978c659dcf0082a9
    lines: 124-167
    snip: ff058a3d3d5e
    anchor: "pub(crate) fn narrow_object_to_instanceof("
  - path: src/types/private_scope.rs
    blob: 369ae8e1bfa9e3413bf6802f157b8cdcc4764b31
    lines: 33-65
    snip: 8b4395f2446d
    anchor: "pub(crate) fn lexical_private_method_scope("
---

# instanceof narrows to the closed world's intersection, and private methods resolve in the lexical scope

## Decision

`$x instanceof I` is an INTERSECTION. `types::instanceof_intersection::narrow_object_to_instanceof`
answers it from the closed world: keep the class if it already satisfies I; take I if I is the more
specific type; otherwise name the declared descendants of the class that satisfy I (cap 4, else the
old behaviour). The checker (`narrowing.rs`) and the IR lowering (`instanceof_branch_local_type`)
both call it -- they narrowed independently before, and disagreeing about `$this` made the SECOND
read of a property fail where the first succeeded (the first was served by a checker-recorded span
type).

MEASURED shapes that were tried first and failed, all on Symfony's `CompiledUrlMatcherTrait::match()`
(`if (!$this instanceof RedirectableUrlMatcherInterface) throw; $this->context->getScheme()`):
- narrow to the interface (old): properties gone, `method call receiver for PHP type Int`;
- keep the class: lost `Kernel::warmUp()` and `ContainerInterface::getParameter()`;
- union class|interface: a union property read needs the property on EVERY member.

Consequence: `$this` is now typed as a DESCENDANT, and PHP resolves `private` by the LEXICAL scope.
`types::private_scope::lexical_private_method_scope` answers "the current class declares it private
and the receiver descends from it"; the checker, the IR signature lookup (which materializes omitted
optional arguments -- missing it gave `2 operands for 4 ABI params`) and codegen's
`resolve_method_call_target` all ask it. It was a standalone gap too: a parent method calling its
own private method on a `new Child()` answered `Undefined method: Child::secret` where php prints the
result. Private PROPERTIES never had it: `inherit_properties` copies them whatever their visibility.
Conservative: only consulted when the receiver's class has NO method of that name.

## Why

Interface-only narrowing lost every property, a union lost them on the interface half; naming descendants works but requires scope-based private resolution
