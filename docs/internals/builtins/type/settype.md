---
title: "settype() — internals"
description: "Compiler internals for settype(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 415
---

## `settype()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:25](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L25) (`lower_settype`)
- **Function symbol**: `lower_settype()`


### Lowering notes

- Lowers `settype($local, "type")` by mutating the resolved local slot and returning true.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function settype(mixed $var, string $type): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$var`.

## Cross-references

- [User reference for `settype()`](../../../php/builtins/type/settype.md)

