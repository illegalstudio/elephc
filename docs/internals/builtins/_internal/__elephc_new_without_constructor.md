---
title: "__elephc_new_without_constructor() — internals"
description: "Compiler internals for __elephc_new_without_constructor(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 444
---

## `__elephc_new_without_constructor()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_new_without_constructor.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_new_without_constructor.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:553](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L553) (`lower_elephc_new_without_constructor`)
- **Function symbol**: `lower_elephc_new_without_constructor()`


### Lowering notes

- Allocates a dynamically named object while deliberately skipping its constructor.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_new_without_constructor(string $class): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
