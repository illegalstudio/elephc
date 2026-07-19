---
title: "fprintf() — internals"
description: "Compiler internals for fprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 171
---

## `fprintf()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fprintf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2863](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2863) (`lower_fprintf`)
- **Function symbol**: `lower_fprintf()`


### Lowering notes

- Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function fprintf(resource $stream, string $format, ...$values): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$values`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fprintf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `fprintf()`](../../../php/builtins/io/fprintf.md)
