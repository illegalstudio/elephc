---
id: bug_fix-a-runtime-declared-subclass-reaches-a-compiled-p-5b66a254
type: bug_fix
title: "A runtime-declared subclass reaches a compiled protected property through the DECLARING class scope"
description: "How the Symfony 192-file --web preload was unblocked without touching the bridge's assembly ABI"
tags: [magician, symfony, web, visibility]
created: 2026-09-16
verified_by: "tests/codegen/regressions/mixed_method_dispatch.rs::test_runtime_declared_subclass_reaches_a_compiled_protected_property; examples/symfony-app 192-file preload answers a byte-identical 404"
sources:
  - path: crates/elephc-magician/src/interpreter/statements/instance_property_access.rs
    blob: 86d2b352d2d10f4ecf250acf8e4bdb9d6d37bcc5
  - path: src/types/checker/type_compat/unions.rs
    blob: aaad114d3c63c67c507460f4254c34da5d8f9af4
supersedes: "gotcha-the-bigger-the-web-preload-the-sooner-an-interpr-e2f0a28b"
---

# A runtime-declared subclass reaches a compiled protected property through the DECLARING class scope

## Fact

The eval property bridge's allow-list is baked into the user assembly from `module.class_infos`, so
a class DECLARED AT RUNTIME that extends a compiled one is in no list and loses protected access.
Every Symfony container fragment is exactly that shape.

The fix does NOT change the bridge's ABI. `eval_property_get_result` and `eval_property_set_result`
already resolve `(declaring_class, visibility)` for an AOT property and already run
`validate_eval_member_access`, which consults BOTH hierarchies (`context.class_is_a` for
interpreted classes, `native_class_is_a` for AOT parent metadata). So the interpreter already knows
the right answer. It now performs the access inside
`eval_reflection_with_declaring_class_scope(&declaring_class, …)` once the check has passed.

Why the declaring class is the right scope to borrow: it is in EVERY allow-list by construction —
`visibility_scope_names` gives `private` the declaring class alone and `protected`
`related_class_scope_names`, which starts from `is_same_or_descendant(D, D)`. So the substitution
grants exactly the access the interpreter just proved, and nothing else. The precedent was two
lines above: reflection-backed public storage already borrows that scope.

One follow-on was needed. `is_callable()` narrowing now yields `callable|string`, and
`type_accepts` decomposes a union member by member, so the `string` member sank it against a
declared `callable` (`cannot initialize $_typedSetter as callable with callable|string`, which
broke three PDO/cairo prelude-pruning tests). A union that CONTAINS `callable` is a value already
PROVEN callable whose shape is merely undecided, so `type_accepts` now admits it whole when every
member is a callable shape. A union WITHOUT a `callable` member keeps the member-by-member rule.
