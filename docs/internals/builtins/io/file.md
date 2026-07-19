---
title: "file() — internals"
description: "Compiler internals for file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 165
---

## `file()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/file.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/file.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3682](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3682) (`lower_file`)
- **Function symbol**: `lower_file()`


### Lowering notes

- Lowers `file(path)` through the target-aware runtime line-array helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file`
- `__rt_realpath`

## Signature summary

```php
function file(string $filename): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `file()`](../../../php/builtins/io/file.md)
