---
title: "ctype_digit() — internals"
description: "Compiler internals for ctype_digit(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 411
---

## `ctype_digit()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/ctype.rs`:25](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/ctype.rs#L25) (`lower_ctype_digit`)
- **Function symbol**: `lower_ctype_digit()`


### Lowering notes

- Lowers `ctype_digit(string)` by checking every byte against the ASCII digit range.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ctype_digit(string $text): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ctype_digit()`](../../../php/builtins/type/ctype_digit.md)

