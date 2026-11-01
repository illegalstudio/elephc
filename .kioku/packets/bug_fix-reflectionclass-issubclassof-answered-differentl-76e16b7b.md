---
id: bug_fix-reflectionclass-issubclassof-answered-differentl-76e16b7b
type: bug_fix
title: "ReflectionClass::isSubclassOf answered differently depending on how its receiver was spelled"
description: "on a local the call goes to the interpreter and is right; on a typed parameter it runs the synthetic body and reads __parent_names, a slot the eval bridge never fills"
created: 2026-09-22
verified_by: "a 60-line repro matching php 8.5.10 on ten spellings; cargo test --release --lib -p elephc keeps its 29-failure baseline"
sources:
  - path: src/types/checker/builtin_types/reflection/class_relations.rs
    blob: 51cdd41d4dedb024284fbedd5f0af3d391374ae2
    lines: 155-350
    snip: 335b6ad7ad24
    anchor: "pub(super) fn builtin_reflection_class_is_subclass_of_method() -> ClassMethod {"
  - path: src/codegen/eval_reflection_owner_helpers.rs
    blob: 5ccd8c45e9f17f863f95563ccfe664c25097b1c4
    lines: 222-245
    snip: b3c37254a00a
    anchor: "fn reflection_owner_layout(info: &ClassInfo, has_name: bool) -> Option<ReflectionOwnerLayout> {"
---

# ReflectionClass::isSubclassOf answered differently depending on how its receiver was spelled

## Fact

`ReflectionClass::isSubclassOf()` is a SYNTHETIC PHP body that walks `$this->__parent_names` and
then `$this->__interface_names`. A reflection object built through the EVAL BRIDGE
(`reflection_owner_layout`) fills `__interface_names`, `__trait_names`, `__method_names`,
`__property_names` and `__parent_class` -- but there is NO `__parent_names` slot in that ABI, so
the array keeps its `[]` default.

That stayed invisible because the call is USUALLY routed to the interpreter: with a local receiver
elephc emits a bridge call and the interpreter answers from its own class table. Only a receiver
whose static type is a typed PARAMETER runs the compiled body. MEASURED, `class PChild extends
PBase`, same object in both lines:

    $r->isSubclassOf(PBase::class)                                 true
    f($r)  where f(\ReflectionClass $r) => $r->isSubclassOf(...)    false

and the INTERFACE arm answered true in both, because its slot is filled. `getName()`,
`getParentClass()` and `hasMethod()` were all correct through the same parameter, which is why
this reads as "reflection works" from any probe that does not ask this one question this one way.

DIAGNOSTIC PATH WORTH REUSING: `ELEPHC_EVAL_TRACE=1` prints `phase=reflection_is_subclass_of` for
the bridge path and NOTHING for the compiled one. A call that produces no trace line is not
"untraced", it is a different lowering.

FIX: walk `$this->__parent_class` upwards in the same body. That slot IS filled on both paths --
by the bridge above and by `member_properties_emit::emit_reflection_parent_class_property` -- and
it holds `false` at the top of a hierarchy, which ends the loop. The `__parent_names` loop is kept
ahead of it so the codegen-materialized path still short-circuits.

RELATED, same session, same file family: `is_a()` and `is_subclass_of()` with a STRING first
argument answered false for every target, because `static_relation_holds` only accepted
`PhpType::Object`. Fixed to accept a constant class-name string, honouring `$allow_string`
(default true for `is_subclass_of`, false for `is_a`). `Foo::class` needed a second fix: it lowers
to `Op::ConstClassName`, not `Op::ConstStr`, and the constant-string accessor only read the
latter. STILL OPEN: a first argument that is a RUNTIME string is not folded and answers false.

## Why

Symfony's AddConsoleCommandPass asks the question inside registerCommand(…, \ReflectionClass $reflection, …), so every console command was refused
