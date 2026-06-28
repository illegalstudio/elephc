---
title: "gzuncompress() — internals"
description: "Compiler internals for gzuncompress(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 332
---

## `gzuncompress()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:457](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L457) (`lower_gzuncompress`)
- **Function symbol**: `lower_gzuncompress()`


### Lowering notes

- Lowers `gzuncompress(data, max_length?)` and boxes zlib failures as PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_long2ip`

## Signature summary

```php
function gzuncompress(string $data, int $max_length): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `gzuncompress()`](../../../php/builtins/string/gzuncompress.md)

