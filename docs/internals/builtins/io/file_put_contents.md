---
title: "file_put_contents() — internals"
description: "Compiler internals for file_put_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 161
---

## `file_put_contents()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3727](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3727) (`lower_file_put_contents`)
- **Function symbol**: `lower_file_put_contents()`


### Lowering notes

- Lowers `file_put_contents(path, data)` through the target-aware runtime writer.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file_put_contents`
- `__rt_file_put_contents_maybe_phar`

## Signature summary

```php
function file_put_contents(string $filename, mixed $data, int $flags = 0, mixed $context = null): int
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `file_put_contents()`](../../../php/builtins/io/file_put_contents.md)

