---
title: "fnmatch() — internals"
description: "Compiler internals for fnmatch(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 116
---

## `fnmatch()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4603](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4603) (`lower_fnmatch`)
- **Function symbol**: `lower_fnmatch()`


### Lowering notes

- Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fnmatch(string $pattern, string $filename, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `fnmatch()`](../../../php/builtins/filesystem/fnmatch.md)

