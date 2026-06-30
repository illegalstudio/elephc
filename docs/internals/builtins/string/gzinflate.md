---
title: "gzinflate() — internals"
description: "Compiler internals for gzinflate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 348
---

## `gzinflate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:436](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L436) (`lower_gzinflate`)
- **Function symbol**: `lower_gzinflate()`


### Lowering notes

- Lowers `gzinflate(data, max_length?)` and boxes zlib failures as PHP false.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gzinflate(string $data, int $max_length): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `gzinflate()`](../../../php/builtins/string/gzinflate.md)

