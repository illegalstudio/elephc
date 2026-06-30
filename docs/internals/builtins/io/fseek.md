---
title: "fseek() — internals"
description: "Compiler internals for fseek(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 169
---

## `fseek()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3172](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3172) (`lower_fseek`)
- **Function symbol**: `lower_fseek()`


### Lowering notes

- Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fseek(resource $stream, int $offset, int $whence): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `fseek()`](../../../php/builtins/io/fseek.md)

