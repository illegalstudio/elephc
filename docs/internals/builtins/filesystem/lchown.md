---
title: "lchown() — internals"
description: "Compiler internals for lchown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 134
---

## `lchown()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/lchown.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/lchown.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4484](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4484) (`lower_lchown`)
- **Function symbol**: `lower_lchown()`


### Lowering notes

- Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function lchown(string $filename, string $user): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/lchown.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lchown.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `lchown()`](../../../php/builtins/filesystem/lchown.md)
