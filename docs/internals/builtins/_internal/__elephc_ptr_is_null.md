---
title: "__elephc_ptr_is_null() — internals"
description: "Compiler internals for __elephc_ptr_is_null(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 469
---

## `__elephc_ptr_is_null()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/__elephc_ptr_is_null.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/__elephc_ptr_is_null.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:51](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L51) (`lower_ptr_is_null`)
- **Function symbol**: `lower_ptr_is_null()`


### Lowering notes

- Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_ptr_is_null(mixed $pointer): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
