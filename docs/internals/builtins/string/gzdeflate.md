---
title: "gzdeflate() — internals"
description: "Compiler internals for gzdeflate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 366
---

## `gzdeflate()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/gzdeflate.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/gzdeflate.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:480](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L480) (`lower_gzdeflate`)
- **Function symbol**: `lower_gzdeflate()`


### Lowering notes

- Lowers `gzdeflate(data, level?)` through inline raw-DEFLATE zlib calls.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gzdeflate(string $data, int $level = -1): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/gzdeflate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/gzdeflate.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gzdeflate()`](../../../php/builtins/string/gzdeflate.md)
