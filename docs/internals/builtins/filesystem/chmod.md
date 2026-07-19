---
title: "chmod() — internals"
description: "Compiler internals for chmod(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 105
---

## `chmod()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/chmod.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/chmod.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4465](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4465) (`lower_chmod`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/chmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chmod.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `chmod()`](../../../php/builtins/filesystem/chmod.md)
