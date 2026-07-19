---
title: "__elephc_class_has_constructor() — internals"
description: "Compiler internals for __elephc_class_has_constructor(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 439
---

## `__elephc_class_has_constructor()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_class_has_constructor.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_class_has_constructor.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:562](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L562) (`lower_elephc_class_has_constructor`)
- **Function symbol**: `lower_elephc_class_has_constructor()`


### Lowering notes

- Tests whether a dynamically named AOT class exposes an inherited or declared constructor.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_class_has_constructor(string $class): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
