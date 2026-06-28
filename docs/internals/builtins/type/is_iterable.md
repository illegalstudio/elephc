---
title: "is_iterable() — internals"
description: "Compiler internals for is_iterable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 406
---

## `is_iterable()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1250](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1250) (`lower_is_iterable`)
- **Function symbol**: `lower_is_iterable()`


### Lowering notes

- Lowers `is_iterable()` for concrete values and boxed Mixed payloads.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_iterable(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_iterable()`](../../../php/builtins/type/is_iterable.md)

