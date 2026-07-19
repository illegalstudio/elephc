---
title: "is_writeable() — internals"
description: "Compiler internals for is_writeable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 132
---

## `is_writeable()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/is_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/is_writeable.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5620](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5620) (`lower_is_writeable`)
- **Function symbol**: `lower_is_writeable()`


### Lowering notes

- Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_link`
- `__rt_is_writable`

## Signature summary

```php
function is_writeable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/is_writeable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_writeable.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_writeable()`](../../../php/builtins/filesystem/is_writeable.md)
