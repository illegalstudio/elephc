---
title: "ctype_alpha() — internals"
description: "Compiler internals for ctype_alpha(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 393
---

## `ctype_alpha()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/ctype.rs`:20](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/ctype.rs#L20) (`lower_ctype_alpha`)
- **Function symbol**: `lower_ctype_alpha()`


### Lowering notes

- Lowers `ctype_alpha(string)` by checking every byte against ASCII alpha ranges.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ctype_alpha(string $text): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ctype_alpha()`](../../../php/builtins/type/ctype_alpha.md)

