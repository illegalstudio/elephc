---
title: "printf() — internals"
description: "Compiler internals for printf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 379
---

## `printf()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/printf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/printf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:535](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L535) (`lower_printf`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/formatting/printf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/printf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `printf()`](../../../php/builtins/string/printf.md)
