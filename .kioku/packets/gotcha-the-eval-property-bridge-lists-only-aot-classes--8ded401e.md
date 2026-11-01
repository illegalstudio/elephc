---
id: gotcha-the-eval-property-bridge-lists-only-aot-classes--8ded401e
type: gotcha
title: "The eval property bridge lists only AOT classes, so an interpreted subclass loses protected access"
description: "The real root cause behind the Symfony --web preload's 'unsupported NullCoalesceAssign', reduced to 26 lines"
tags: [magician, codegen, symfony, web]
created: 2026-09-16
verified_by: "scratchpad/ncaprobe.php (fails) vs scratchpad/ncapublic.php (identical to php -n); the only difference is protected vs public"
sources:
  - path: src/codegen/eval_property_helpers.rs
    blob: 4c5012548c3bf48f9104e6dec7567bf5718d9e35
  - path: crates/elephc-magician/src/runtime_hooks/ops/collection_calls.rs
    blob: 2d067f7da99a424872c0ac9ef7efae674f57d8c4
superseded_by: "bug_fix-a-runtime-declared-subclass-reaches-a-compiled-p-5b66a254"
---

# The eval property bridge lists only AOT classes, so an interpreted subclass loses protected access

## Fact

`__elephc_eval_value_property_get/set` compare the interpreter's current class scope against a
STATIC allow-list baked into the user assembly. `visibility_scope_names` builds it from
`related_class_scope_names`, which iterates `module.class_infos` — the AOT classes and nothing else.

A class DECLARED AT RUNTIME that extends a compiled one is therefore not in the list, and every
access it makes to a protected (or private) property of a compiled object is refused. The refusal
carries no description, so it surfaces as the outermost expression kind:

    Fatal error: eval() runtime failed: unsupported PropertyGet expression

This is what kills the 192-file Symfony preload. Symfony's container fragments live in `var/cache`,
are `require`d at runtime, and each declares
`class getXService extends App_KernelProdContainer` — an interpreted subclass of the compiled
container — whose body reads `$container->services[...]`, declared `protected array $services` on
`Symfony\Component\DependencyInjection\Container`. The first such access happens to be inside a
`??=`, which is why the message named `NullCoalesceAssign`; a PLAIN READ fails identically.

Reduction (scratchpad/ncaprobe.php + ncafrag/frag.php): compiled `BaseContainer` with
`protected array $services`, compiled `AppContainer extends BaseContainer`, and a runtime-required
`FragRunner extends AppContainer` whose static method reads `$container->services`. Flip the two
properties to `public` (scratchpad/ncapublic.php) and it matches `php -n` exactly.

Fixing it needs the scope check to fall back to a RUNTIME answer when the scope name is not in the
static list — the interpreter knows the hierarchy of the classes it declared. Substituting the
nearest AOT ancestor on the magician side is sound for `protected` but would WIDEN `private`
(a child must not see its parent's private), so the two visibilities cannot share one scope string.
