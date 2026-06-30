---
title: "is_writeable() — internals"
description: "Compiler internals for is_writeable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 126
---

## `is_writeable()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5624](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5624) (`lower_is_writeable`)
- **Function symbol**: `lower_is_writeable()`


### Lowering notes

- Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_link`
- `__rt_is_writable`

## Signature summary

```php
function is_writeable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_writeable()`](../../../php/builtins/filesystem/is_writeable.md)

