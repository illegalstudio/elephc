---
title: "flock() — internals"
description: "Compiler internals for flock(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 162
---

## `flock()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3309](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3309) (`lower_flock`)
- **Function symbol**: `lower_flock()`


### Lowering notes

- Lowers `flock(stream, operation, would_block?)` through the libc flock wrapper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function flock(resource $stream, int $operation, bool $would_block): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$would_block`.

## Cross-references

- [User reference for `flock()`](../../../php/builtins/io/flock.md)

