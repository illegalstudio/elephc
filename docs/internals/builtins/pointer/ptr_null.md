---
title: "ptr_null() — internals"
description: "Compiler internals for ptr_null(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 291
---

## `ptr_null()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:49](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L49) (`lower_ptr_null`)
- **Function symbol**: `lower_ptr_null()`


### Lowering notes

- Lowers `ptr_null()` by materializing the raw null pointer sentinel.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_null(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `ptr_null()`](../../../php/builtins/pointer/ptr_null.md)

