---
title: "pathinfo() — internals"
description: "Compiler internals for pathinfo(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 139
---

## `pathinfo()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/pathinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/pathinfo.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4644](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4644) (`lower_pathinfo`)
- **Function symbol**: `lower_pathinfo()`


### Lowering notes

- Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_pathinfo_array`

## Signature summary

```php
function pathinfo(string $path, int $flags = 15): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/pathinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pathinfo.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pathinfo()`](../../../php/builtins/filesystem/pathinfo.md)
