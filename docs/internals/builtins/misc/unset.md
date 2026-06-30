---
title: "unset() — internals"
description: "Compiler internals for unset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 286
---

## `unset()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:48](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L48) (`lower_unset_builtin`)
- **Function symbol**: `lower_unset_builtin()`


### Lowering notes

- Rejects `unset()` calls that were not converted into direct EIR unbind operations.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function unset(mixed $var, ...$vars): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `unset()`](../../../php/builtins/misc/unset.md)

