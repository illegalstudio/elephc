---
title: "__elephc_normalize_callable() — internals"
description: "Compiler internals for __elephc_normalize_callable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 445
---

## `__elephc_normalize_callable()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/elephc_normalize_callable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/elephc_normalize_callable.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:170](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L170) (`lower_elephc_normalize_callable`)
- **Function symbol**: `lower_elephc_normalize_callable()`


### Lowering notes

- Lowers `__elephc_normalize_callable($cb)` into an owned callable descriptor.
- Static descriptors tolerate retains as persistent values, while runtime descriptors
- selected from an existing `Callable` or boxed callable need one additional owner for
- the returned value. Fresh receiver-bound descriptors already start with one owner.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_normalize_callable(mixed $value): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
