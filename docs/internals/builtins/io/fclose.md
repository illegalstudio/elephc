---
title: "fclose() — internals"
description: "Compiler internals for fclose(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 152
---

## `fclose()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2707](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2707) (`lower_fclose`)
- **Function symbol**: `lower_fclose()`


### Lowering notes

- Lowers `fclose(stream)` after validating and unboxing the stream handle.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fclose(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fclose()`](../../../php/builtins/io/fclose.md)

