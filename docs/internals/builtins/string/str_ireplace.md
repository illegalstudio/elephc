---
title: "str_ireplace() — internals"
description: "Compiler internals for str_ireplace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 379
---

## `str_ireplace()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/str_ireplace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/str_ireplace.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:780](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L780) (`lower_string_replace`)
- **Function symbol**: `lower_string_replace()`


### Lowering notes

- Lowers `str_replace()`/`str_ireplace()` with three string operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function str_ireplace(string $search, string $replace, string $subject, int $count = null): string
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).

## Cross-references

- [User reference for `str_ireplace()`](../../../php/builtins/string/str_ireplace.md)
