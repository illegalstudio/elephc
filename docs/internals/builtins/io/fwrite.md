---
title: "fwrite() — internals"
description: "Compiler internals for fwrite(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 176
---

## `fwrite()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fwrite.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fwrite.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2838](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2838) (`lower_fwrite`)
- **Function symbol**: `lower_fwrite()`


### Lowering notes

- Lowers `fwrite(stream, data)` and returns the number of bytes written.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fwrite`

## Signature summary

```php
function fwrite(resource $stream, string $data): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `fwrite()`](../../../php/builtins/io/fwrite.md)
