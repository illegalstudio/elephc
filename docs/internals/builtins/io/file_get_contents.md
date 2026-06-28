---
title: "file_get_contents() — internals"
description: "Compiler internals for file_get_contents(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 145
---

## `file_get_contents()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:39](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L39) (`lower_file_get_contents`)
- **Function symbol**: `lower_file_get_contents()`


### Lowering notes

- Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file_get_contents_maybe_url`
- `__rt_php_input`

## Signature summary

```php
function file_get_contents(string $filename, bool $use_include_path, mixed $context, int $offset, int $length): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.

## Cross-references

- [User reference for `file_get_contents()`](../../../php/builtins/io/file_get_contents.md)

