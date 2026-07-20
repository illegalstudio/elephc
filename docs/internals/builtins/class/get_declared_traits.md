---
title: "get_declared_traits() — internals"
description: "Compiler internals for get_declared_traits(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 82
---

## `get_declared_traits()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/get_declared_traits.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/get_declared_traits.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:395](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L395) (`lower_get_declared_names`)
- **Function symbol**: `lower_get_declared_names()`


### Lowering notes

- Lowers `get_declared_classes/interfaces/traits()` using the shared declaration registry.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_declared_traits(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_traits.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_traits.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_declared_traits()`](../../../php/builtins/class/get_declared_traits.md)
