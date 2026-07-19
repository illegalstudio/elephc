---
title: "fnmatch() — internals"
description: "Compiler internals for fnmatch(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 122
---

## `fnmatch()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fnmatch.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fnmatch.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4599](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4599) (`lower_fnmatch`)
- **Function symbol**: `lower_fnmatch()`


### Lowering notes

- Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fnmatch(string $pattern, string $filename, int $flags = 0): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fnmatch.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fnmatch.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fnmatch()`](../../../php/builtins/filesystem/fnmatch.md)
