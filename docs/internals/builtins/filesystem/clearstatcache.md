---
title: "clearstatcache() — internals"
description: "Compiler internals for clearstatcache(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 107
---

## `clearstatcache()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/clearstatcache.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/clearstatcache.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5574](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5574) (`lower_clearstatcache`)
- **Function symbol**: `lower_clearstatcache()`


### Lowering notes

- Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_dir`

## Signature summary

```php
function clearstatcache(bool $clear_realpath_cache = false, string $filename = ''): void
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/clearstatcache.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/clearstatcache.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `clearstatcache()`](../../../php/builtins/filesystem/clearstatcache.md)
