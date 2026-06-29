---
title: "clearstatcache() — internals"
description: "Compiler internals for clearstatcache(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 86
---

## `clearstatcache()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5572](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5572) (`lower_clearstatcache`)
- **Function symbol**: `lower_clearstatcache()`


### Lowering notes

- Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_dir`

## Signature summary

```php
function clearstatcache(bool $clear_realpath_cache, string $filename): void
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Cross-references

- [User reference for `clearstatcache()`](../../../php/builtins/filesystem/clearstatcache.md)

