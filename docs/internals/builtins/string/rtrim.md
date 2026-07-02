---
title: "rtrim() — internals"
description: "Compiler internals for rtrim(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 373
---

## `rtrim()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/rtrim.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/rtrim.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:112](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L112) (`lower_trim_like`)
- **Function symbol**: `lower_trim_like()`


### Lowering notes

- Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function rtrim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `rtrim()`](../../../php/builtins/string/rtrim.md)

