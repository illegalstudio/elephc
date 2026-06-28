---
title: "isset() — internals"
description: "Compiler internals for isset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 265
---

## `isset()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/isset.rs`:24](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/isset.rs#L24) (`lower_isset`)
- **Function symbol**: `lower_isset()`


### Lowering notes

- Lowers `isset()` for values already evaluated by the EIR frontend.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function isset(mixed $var, ...$vars): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `isset()`](../../../php/builtins/misc/isset.md)

