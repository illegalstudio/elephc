---
title: "gettype() — internals"
description: "Compiler internals for gettype(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 436
---

## `gettype()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/gettype.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/gettype.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:418](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L418) (`lower_gettype`)
- **Function symbol**: `lower_gettype()`


### Lowering notes

- Lowers `gettype(value)` for statically concrete PHP types.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gettype(mixed $value): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/gettype.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/gettype.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gettype()`](../../../php/builtins/type/gettype.md)
