---
title: "ltrim() — internals"
description: "Compiler internals for ltrim(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 353
---

## `ltrim()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:112](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L112) (`lower_trim_like`)
- **Function symbol**: `lower_trim_like()`


### Lowering notes

- Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ltrim(string $string, string $characters): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `ltrim()`](../../../php/builtins/string/ltrim.md)

