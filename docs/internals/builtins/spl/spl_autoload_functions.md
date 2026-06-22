---
title: "spl_autoload_functions() — internals"
description: "Compiler internals for spl_autoload_functions(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 310
---

## `spl_autoload_functions()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:166](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L166) (`lower_spl_autoload_functions`)
- **Function symbol**: `lower_spl_autoload_functions()`


### Lowering notes

- Lowers `spl_autoload_functions()` to an indexed array of AOT rule placeholders.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload_functions(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `spl_autoload_functions()`](../../../php/builtins/spl/spl_autoload_functions.md)

