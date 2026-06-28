---
title: "gzdeflate() — internals"
description: "Compiler internals for gzdeflate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 330
---

## `gzdeflate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:418](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L418) (`lower_gzdeflate`)
- **Function symbol**: `lower_gzdeflate()`


### Lowering notes

- Lowers `gzdeflate(data, level?)` through inline raw-DEFLATE zlib calls.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gzdeflate(string $data, int $level, int $encoding): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `gzdeflate()`](../../../php/builtins/string/gzdeflate.md)

