---
title: "var_dump() — internals"
description: "Compiler internals for var_dump(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 288
---

## `var_dump()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/var_dump.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/var_dump.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/debug.rs`:152](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/debug.rs#L152) (`lower_var_dump`)
- **Function symbol**: `lower_var_dump()`


### Lowering notes

- Lowers `var_dump(value, ...values)` for concrete scalar/resource values and array/hash shells.
- Each operand is dumped independently in source order, matching PHP's variadic var_dump.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function var_dump(mixed $value, ...$values): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/var_dump.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/var_dump.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `var_dump()`](../../../php/builtins/misc/var_dump.md)
