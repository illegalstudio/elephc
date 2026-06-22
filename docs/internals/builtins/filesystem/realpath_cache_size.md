---
title: "realpath_cache_size() — internals"
description: "Compiler internals for realpath_cache_size(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 124
---

## `realpath_cache_size()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3704](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3704) (`lower_realpath_cache_size`)
- **Function symbol**: `lower_realpath_cache_size()`


### Lowering notes

- Lowers `realpath_cache_size()` to zero because elephc has no realpath cache.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function realpath_cache_size(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `realpath_cache_size()`](../../../php/builtins/filesystem/realpath_cache_size.md)

