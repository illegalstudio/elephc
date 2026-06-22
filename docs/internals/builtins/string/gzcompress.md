---
title: "gzcompress() — internals"
description: "Compiler internals for gzcompress(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 331
---

## `gzcompress()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:402](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L402) (`lower_gzcompress`)
- **Function symbol**: `lower_gzcompress()`


### Lowering notes

- Lowers `gzcompress(data, level?)` through inline zlib `compress2` calls.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gzcompress(string $data, int $level, int $encoding): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `gzcompress()`](../../../php/builtins/string/gzcompress.md)

