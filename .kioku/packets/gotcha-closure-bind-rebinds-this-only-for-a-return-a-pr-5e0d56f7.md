---
id: gotcha-closure-bind-rebinds-this-only-for-a-return-a-pr-5e0d56f7
type: gotcha
title: "Closure::bind rebinds $this only for a return-a-property closure body"
description: "closure_bind_property_receiver_type only matches a body of exactly return $this->prop, so every other bound closure keeps the enclosing class $this in both the checker and codegen"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/objects/methods.rs
    blob: 842fc1265b20dcf0712da009ca34c2279f8fc4ec
superseded_by: "gotcha-the-runtime-closure-bind-does-not-write-the-new--82153f10"
superseded_why: "Same blocker, but the root cause is now known: both type sides were widened together and the program fataled at run time with an empty receiver class"
---

# Closure::bind rebinds $this only for a return-a-property closure body

## Fact

`closure_bind_property_receiver_type` (src/types/checker/inference/objects/methods.rs) implements
`Closure::bind($closure, $newThis, $scope)`'s `$this` rebinding, but only recognises a closure whose
body is EXACTLY one statement of the form `return $this->prop;`. Any other body keeps `$this` typed
as the ENCLOSING class.

Symfony's `RedisTrait` line 70 is the counter-example:

    \Closure::bind(function () { $this->options['exceptions'] = false; }, $options, $options)();

which reports `Undefined property: Symfony\Component\Cache\Adapter\RedisAdapter::options` — a class
that has no such property and is not the bind target.

Reduced, the same shape reaches the BACKEND instead:
`unsupported EIR backend feature: runtime_call property array set with receiver PHP type Object("User")`.
That is the reason the checker fix alone is not enough: codegen also types `$this` in the closure
body as the enclosing class, so checker and backend must be changed together or they disagree.

## Why

The narrow shape check silently keeps the enclosing class's $this for every other body
