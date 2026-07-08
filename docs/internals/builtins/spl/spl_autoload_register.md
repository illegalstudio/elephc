---
title: "spl_autoload_register() — internals"
description: "Compiler internals for spl_autoload_register(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 327
---

## `spl_autoload_register()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_autoload_register.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_autoload_register.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:135](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L135) (`lower_spl_autoload_bool`)
- **Function symbol**: `lower_spl_autoload_bool()`


### Lowering notes

- Lowers autoload registration stubs by preserving arg effects and returning true.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload_register(callable $callback = null, bool $throw = true, bool $prepend = false): bool
```

## What the type checker enforces

- **Arity**: takes 0–3 arguments (3 optional).

## Cross-references

- [User reference for `spl_autoload_register()`](../../../php/builtins/spl/spl_autoload_register.md)
