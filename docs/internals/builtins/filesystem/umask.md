---
title: "umask() — internals"
description: "Compiler internals for umask(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 155
---

## `umask()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/umask.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/umask.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4489](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4489) (`lower_umask`)
- **Function symbol**: `lower_umask()`


### Lowering notes

- Lowers `umask(mask?)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function umask(int $mask = null): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/umask.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/umask.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `umask()`](../../../php/builtins/filesystem/umask.md)
