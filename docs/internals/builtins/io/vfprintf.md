---
title: "vfprintf() — internals"
description: "Compiler internals for vfprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 212
---

## `vfprintf()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2677](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2677) (`lower_vfprintf`)
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

## Cross-references

- [User reference for `vfprintf()`](../../../php/builtins/io/vfprintf.md)

