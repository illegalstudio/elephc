---
title: "rewind() — internals"
description: "Compiler internals for rewind(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 170
---

## `rewind()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3194](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3194) (`lower_rewind`)
- **Function symbol**: `lower_rewind()`


### Lowering notes

- Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function rewind(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `rewind()`](../../../php/builtins/io/rewind.md)

