---
title: "unregister_tick_function() - internals"
description: "Compiler internals for unregister_tick_function(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 794
---

## `unregister_tick_function()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: unavailable for this backend.
- **Function symbol**: none.


## Semantic descriptor

Shared contract intentionally unsupported by the AOT backend.

## EIR and runtime boundary

_No compiled lowering: this surface is intentionally eval-only._

## Signature summary

```php
function unregister_tick_function(mixed $callback): void
```

## Signature constraints

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/tick_functions.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `unregister_tick_function()`](../../../php/builtins/misc/unregister_tick_function.md)
