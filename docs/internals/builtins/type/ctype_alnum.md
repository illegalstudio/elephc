---
title: "ctype_alnum() — internals"
description: "Compiler internals for ctype_alnum(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 392
---

## `ctype_alnum()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/ctype.rs`:30](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/ctype.rs#L30) (`lower_ctype_alnum`)
- **Function symbol**: `lower_ctype_alnum()`


### Lowering notes

- Lowers `ctype_alnum(string)` by checking every byte against ASCII alpha or digit ranges.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ctype_alnum(string $text): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ctype_alnum()`](../../../php/builtins/type/ctype_alnum.md)

