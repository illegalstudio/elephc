---
title: "ptr() — internals"
description: "Compiler internals for ptr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 273
---

## `ptr()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/pointers.rs`:25](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/pointers.rs#L25) (`lower_ptr`)
- **Function symbol**: `lower_ptr()`


### Lowering notes

- Lowers `ptr(value)` by materializing the address of addressable local/global storage.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ptr(mixed $value): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ptr()`](../../../php/builtins/pointer/ptr.md)

