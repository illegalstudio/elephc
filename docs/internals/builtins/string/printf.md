---
title: "printf() — internals"
description: "Compiler internals for printf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 373
---

## `printf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:517](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L517) (`lower_printf`)
- **Function symbol**: `lower_printf()`


### Lowering notes

- Lowers `printf(format, values...)` as `sprintf()` followed by stdout emission.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function printf(string $format, ...$values): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `printf()`](../../../php/builtins/string/printf.md)

