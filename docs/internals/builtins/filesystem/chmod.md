---
title: "chmod() — internals"
description: "Compiler internals for chmod(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 84
---

## `chmod()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3825](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3825) (`lower_chmod`)
- **Function symbol**: `lower_chmod()`


### Lowering notes

- Lowers `chmod(path, mode)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function chmod(string $filename, int $permissions): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `chmod()`](../../../php/builtins/filesystem/chmod.md)

