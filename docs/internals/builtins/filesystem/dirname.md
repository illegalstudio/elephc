---
title: "dirname() — internals"
description: "Compiler internals for dirname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 109
---

## `dirname()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/dirname.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/dirname.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4576](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4576) (`lower_dirname`)
- **Function symbol**: `lower_dirname()`


### Lowering notes

- Lowers `dirname(path, levels?)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_dirname`
- `__rt_dirname_levels`

## Signature summary

```php
function dirname(string $path, int $levels = 1): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/dirname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/dirname.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `dirname()`](../../../php/builtins/filesystem/dirname.md)
