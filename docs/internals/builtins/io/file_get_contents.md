---
title: "file_get_contents() — internals"
description: "Compiler internals for file_get_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 166
---

## `file_get_contents()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/file_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/file_get_contents.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:38](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L38) (`lower_file_get_contents`)
- **Function symbol**: `lower_file_get_contents()`


### Lowering notes

- Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file_get_contents_maybe_url`
- `__rt_php_input`

## Signature summary

```php
function file_get_contents(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `file_get_contents()`](../../../php/builtins/io/file_get_contents.md)
