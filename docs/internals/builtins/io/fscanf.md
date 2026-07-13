---
title: "fscanf() — internals"
description: "Compiler internals for fscanf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 174
---

## `fscanf()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fscanf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2938](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2938) (`lower_fscanf`)
- **Function symbol**: `lower_fscanf()`


### Lowering notes

- Lowers `fscanf(stream, format)` through `__rt_fgets` and `__rt_sscanf`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fgets`
- `__rt_sscanf`

## Signature summary

```php
function fscanf(resource $stream, string $format, ...$vars): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$vars`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `fscanf()`](../../../php/builtins/io/fscanf.md)
