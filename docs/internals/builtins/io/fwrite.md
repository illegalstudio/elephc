---
title: "fwrite() — internals"
description: "Compiler internals for fwrite(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 159
---

## `fwrite()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2836](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2836) (`lower_fwrite`)
- **Function symbol**: `lower_fwrite()`


### Lowering notes

- Lowers `fwrite(stream, data)` and returns the number of bytes written.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fwrite`

## Signature summary

```php
function fwrite(resource $stream, string $data, int $length): int
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `fwrite()`](../../../php/builtins/io/fwrite.md)

