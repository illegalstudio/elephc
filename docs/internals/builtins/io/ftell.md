---
title: "ftell() — internals"
description: "Compiler internals for ftell(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 178
---

## `ftell()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ftell.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ftell.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3134](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3134) (`lower_ftell`)
- **Function symbol**: `lower_ftell()`


### Lowering notes

- Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_user_wrapper_ftell`

## Signature summary

```php
function ftell(resource $stream): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/ftell.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/ftell.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ftell()`](../../../php/builtins/io/ftell.md)
