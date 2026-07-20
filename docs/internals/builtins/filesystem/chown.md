---
title: "chown() — internals"
description: "Compiler internals for chown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 106
---

## `chown()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/chown.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/chown.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4469](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4469) (`lower_chown`)
- **Function symbol**: `lower_chown()`


### Lowering notes

- Lowers `chown(path, owner)` for integer UIDs and string user names.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function chown(string $filename, string $user): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/chown.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chown.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `chown()`](../../../php/builtins/filesystem/chown.md)
