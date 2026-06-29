---
title: "strrpos() — internals"
description: "Compiler internals for strrpos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 379
---

## `strrpos()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:700](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L700) (`lower_string_position`)
- **Function symbol**: `lower_string_position()`


### Lowering notes

- Lowers `strpos()`/`strrpos()` and boxes position-or-false results as Mixed.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function strrpos(string $haystack, string $needle, int $offset): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `strrpos()`](../../../php/builtins/string/strrpos.md)

