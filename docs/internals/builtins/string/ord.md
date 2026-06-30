---
title: "ord() — internals"
description: "Compiler internals for ord(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 372
---

## `ord()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:834](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L834) (`lower_ord`)
- **Function symbol**: `lower_ord()`


### Lowering notes

- Lowers `ord()` by returning the first byte of a string or zero for empty input.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ord(string $character): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ord()`](../../../php/builtins/string/ord.md)

