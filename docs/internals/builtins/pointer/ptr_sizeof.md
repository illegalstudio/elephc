---
title: "ptr_sizeof() — internals"
description: "Compiler internals for ptr_sizeof(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 281
---

## `ptr_sizeof()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:75](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L75) (`lower_ptr_sizeof`)
- **Function symbol**: `lower_ptr_sizeof()`


### Lowering notes

- Lowers `ptr_sizeof("type")` by materializing the checked static byte size.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr_sizeof(string $type): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ptr_sizeof()`](../../../php/builtins/pointer/ptr_sizeof.md)

