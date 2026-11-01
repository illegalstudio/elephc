---
id: bug_fix-a-union-receiver-picked-its-callee-signature-fro-fddc7c8f
type: bug_fix
title: "A union receiver picked its callee signature from the whole program"
description: "a method call on a union-typed receiver lost its omitted optional arguments because an unrelated class anywhere declared the same method name differently"
created: 2026-09-22
verified_by: "five standalone repros byte-identical to php 8.5.10; cargo test --release --lib -p elephc keeps its 29-failure baseline"
sources:
  - path: src/ir_lower/expr/method_metadata.rs
    blob: f3638f01349fa231a7ec958a58a490065ba084fa
---

# A union receiver picked its callee signature from the whole program

## Fact

`method_signature` sent a union-typed receiver to `common_dynamic_method_signature`, which scans
EVERY class in the module for the method name and returns None as soon as two disagree. The
receiver's own union members were never consulted. With no signature the call lowered without
materializing the callee's omitted optional arguments, and codegen -- which still resolved the
callee nominally -- met the arity mismatch and reported `Call to a member function m() on null`.

DECISIVE EXPERIMENT (60 lines, no framework): `class A` and `class B` both declare
`m(?string $n = null, int $f = 7)`, and `function f(A|B $o) { return $o->m(); }` answers
`A:NULL/7` like php. Adding `class Unrelated { public function m(int $onlyOne = 3) {} }` to the
same file -- never called, not in the union -- turns it into `Call to a member function m() on
null`. Removing it restores the right answer. The bug is reachable by adding a class you never
call.

Symfony's console hit it on `AttributeAutoconfigurationPass::callConfigurators`, whose
`\ReflectionClass|\ReflectionMethod|\ReflectionParameter|\ReflectionProperty $reflector` lost
`getAttributes()`'s two optional parameters. Reading the parameter (`get_class($r)`) worked;
only the method CALL failed, which is why an element-read probe would have said the feature works.

FIX: ask the union's own object members first, skipping members that do not declare the method
(they can never be the callee) and bailing to the program-wide answer on a `Mixed` member. A bare
`object` parameter is `Object("")` -- one object type with no class -- and now takes the dynamic
answer too, which fixes `f(object $o) { $o->m(); }` reporting `Call to undefined method A::m()`.

STILL OPEN: an `object`-typed receiver holding a BUILTIN reflection object still answers
`Call to undefined method ReflectionClass::getAttributes()`; the dynamic dispatch table does not
carry the synthetic reflection methods.

## Why

An unrelated class with the same method name and a different signature silently broke every call through a union-typed receiver
