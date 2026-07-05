---
title: "intval() — internals"
description: "Compiler internals for intval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 414
---

## `intval()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/intval.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/intval.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:524](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L524) (`lower_intval`)
- **Function symbol**: `lower_intval()`


### Lowering notes

- Lowers `intval()` for concrete scalar operands.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_int`
- `__rt_str_to_int`

## Signature summary

```php
function intval(mixed $value): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `intval()`](../../../php/builtins/type/intval.md)
