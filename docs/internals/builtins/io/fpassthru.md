---
title: "fpassthru() — internals"
description: "Compiler internals for fpassthru(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 170
---

## `fpassthru()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fpassthru.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fpassthru.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3027](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3027) (`lower_fpassthru`)
- **Function symbol**: `lower_fpassthru()`


### Lowering notes

- Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_feof`
- `__rt_fpassthru`

## Signature summary

```php
function fpassthru(resource $stream): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fpassthru.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fpassthru.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fpassthru()`](../../../php/builtins/io/fpassthru.md)
