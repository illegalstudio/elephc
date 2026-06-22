---
title: "str_replace() — internals"
description: "Compiler internals for str_replace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 370
---

## `str_replace()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:780](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L780) (`lower_string_replace`)
- **Function symbol**: `lower_string_replace()`


### Lowering notes

- Lowers `str_replace()`/`str_ireplace()` with three string operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function str_replace(string $search, string $replace, string $subject, int $count): mixed
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `str_replace()`](../../../php/builtins/string/str_replace.md)

