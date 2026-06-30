---
title: "realpath_cache_get() — internals"
description: "Compiler internals for realpath_cache_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 138
---

## `realpath_cache_get()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3700](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3700) (`lower_realpath_cache_get`)
- **Function symbol**: `lower_realpath_cache_get()`


### Lowering notes

- Lowers `realpath_cache_get()` to elephc's empty realpath-cache view.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function realpath_cache_get(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `realpath_cache_get()`](../../../php/builtins/filesystem/realpath_cache_get.md)

