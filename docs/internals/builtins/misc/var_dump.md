---
title: "var_dump() — internals"
description: "Compiler internals for var_dump(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 284
---

## `var_dump()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/var_dump.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/var_dump.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/debug.rs`:35](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/debug.rs#L35) (`lower_var_dump`)
- **Function symbol**: `lower_var_dump()`


### Lowering notes

- Lowers `var_dump(value)` for concrete scalar/resource values and array/hash shells.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function var_dump(mixed $value): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `var_dump()`](../../../php/builtins/misc/var_dump.md)
