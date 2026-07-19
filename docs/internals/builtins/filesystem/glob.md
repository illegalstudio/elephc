---
title: "glob() — internals"
description: "Compiler internals for glob(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 125
---

## `glob()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/glob.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/glob.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4463](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4463) (`lower_glob`)
- **Function symbol**: `lower_glob()`


### Lowering notes

- Lowers `glob(pattern)` through the target-aware runtime glob expansion helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`

## Signature summary

```php
function glob(string $pattern): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/glob.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/glob.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `glob()`](../../../php/builtins/filesystem/glob.md)
