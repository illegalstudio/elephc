---
id: gotcha-a-by-reference-argument-does-not-define-its-vari-f01f8d26
type: gotcha
title: "A by-reference argument does not define its variable when the receiver is a X|false union"
description: "The 12-of-26 checker errors blocking symfony/cache from the preload, reduced to 20 lines, and what a partial fix did not cover"
tags: [checker, by-ref, symfony]
created: 2026-09-16
verified_by: "scratchpad/byrefunion3.php against php -n; the partial fix was reverted after it failed to move the max-preload count"
sources:
  - path: src/types/checker/inference/expr/effects.rs
    blob: 12eb6ec03d1f341a7b5db5c14e888ad7cf46aa3a
superseded_by: "bug_fix-a-by-ref-argument-defines-its-variable-through-a-93d54af6"
superseded_why: "The union was not the cause: the callable resolver answered before the class schema with a one-parameter non-ref signature, and PDOStatement was in self.classes all along"
---

# A by-reference argument does not define its variable when the receiver is a X|false union

## Fact

Passing a variable to a BY-REFERENCE parameter DEFINES it — PHP binds the slot at the call and
creates it as null. elephc does that for a plainly-typed receiver, and NOT when the receiver is a
union:

    function q(PDOStatement $stmt)        { $stmt->bindParam(':id', $id); }  // fine
    function p(PDOStatement|false $stmt)  { $stmt->bindParam(':id', $id); }  // Undefined variable: $id
    function u(Holder|false $h)           { $h->bind('k', $id); }            // same, user class

`prepare()` returns `PDOStatement|false`, so Symfony's `PdoAdapter` hits it on every
`bindParam(...)` — twelve of the twenty-six errors that keep `symfony/cache` out of the maximal
preload.

Located: `instance_call_effect_signature` (effects.rs) resolves a signature only from
`PhpType::Object`, so a union receiver yields None and `prepare_by_ref_variable_storage` never
runs.

A PARTIAL fix — resolving the union through `union_single_object_class` there — was tried and
REVERTED. It silenced the call site but not the later READ of the variable, and did nothing at all
for `PDOStatement`, which is not in `self.classes` (prelude classes are registered elsewhere). Two
more pieces are needed: whatever makes the plain path record the definition for the subsequent
read, and the prelude-class lookup.
