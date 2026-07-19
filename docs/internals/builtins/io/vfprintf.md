---
title: "vfprintf() — internals"
description: "Compiler internals for vfprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 246
---

## `vfprintf()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/vfprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/vfprintf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2897](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2897) (`lower_vfprintf`)
- **Function symbol**: `lower_vfprintf()`


### Lowering notes

- Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fwrite`
- `__rt_vsprintf`

## Signature summary

```php
function vfprintf(resource $stream, string $format, array $values): int
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/vfprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/vfprintf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `vfprintf()`](../../../php/builtins/io/vfprintf.md)
