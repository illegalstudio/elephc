---
title: "rewinddir() — internals"
description: "Compiler internals for rewinddir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 192
---

## `rewinddir()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/rewinddir.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/rewinddir.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3589](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3589) (`lower_rewinddir`)
- **Function symbol**: `lower_rewinddir()`


### Lowering notes

- Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_rewinddir`
- `__rt_user_wrapper_dir_rewinddir`

## Signature summary

```php
function rewinddir(resource $dir_handle): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/rewinddir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rewinddir.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `rewinddir()`](../../../php/builtins/io/rewinddir.md)
