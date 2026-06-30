---
title: "spl_autoload_unregister() — internals"
description: "Compiler internals for spl_autoload_unregister(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 327
---

## `spl_autoload_unregister()` — internals

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
function spl_autoload_unregister(callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `spl_autoload_unregister()`](../../../php/builtins/spl/spl_autoload_unregister.md)

