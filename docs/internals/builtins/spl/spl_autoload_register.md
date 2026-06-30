---
title: "spl_autoload_register() — internals"
description: "Compiler internals for spl_autoload_register(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 326
---

## `spl_autoload_register()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:134](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L134) (`lower_spl_autoload_bool`)
- **Function symbol**: `lower_spl_autoload_bool()`


### Lowering notes

- Lowers autoload registration stubs by preserving arg effects and returning true.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload_register(callable $callback, bool $throw, bool $prepend): bool
```

## What the type checker enforces

- **Arity**: takes 0–3 arguments (3 optional).

## Cross-references

- [User reference for `spl_autoload_register()`](../../../php/builtins/spl/spl_autoload_register.md)

