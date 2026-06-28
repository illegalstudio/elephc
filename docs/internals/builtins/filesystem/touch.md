---
title: "touch() — internals"
description: "Compiler internals for touch(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 133
---

## `touch()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3880](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3880) (`lower_touch`)
- **Function symbol**: `lower_touch()`


### Lowering notes

- Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function touch(string $filename, int $mtime, int $atime): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `touch()`](../../../php/builtins/filesystem/touch.md)

