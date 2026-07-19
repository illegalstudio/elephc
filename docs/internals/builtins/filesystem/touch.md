---
title: "touch() — internals"
description: "Compiler internals for touch(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 154
---

## `touch()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/touch.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/touch.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4523](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4523) (`lower_touch`)
- **Function symbol**: `lower_touch()`


### Lowering notes

- Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function touch(string $filename, int $mtime = null, int $atime = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/touch.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/touch.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `touch()`](../../../php/builtins/filesystem/touch.md)
