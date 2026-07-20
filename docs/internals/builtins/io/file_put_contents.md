---
title: "file_put_contents() — internals"
description: "Compiler internals for file_put_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 167
---

## `file_put_contents()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/file_put_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/file_put_contents.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3723](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3723) (`lower_file_put_contents`)
- **Function symbol**: `lower_file_put_contents()`


### Lowering notes

- Lowers `file_put_contents(path, data)` through the target-aware runtime writer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file_put_contents`
- `__rt_file_put_contents_maybe_phar`

## Signature summary

```php
function file_put_contents(string $filename, string $data): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/file_put_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_put_contents.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `file_put_contents()`](../../../php/builtins/io/file_put_contents.md)
