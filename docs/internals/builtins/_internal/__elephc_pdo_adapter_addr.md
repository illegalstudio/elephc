---
title: "__elephc_pdo_adapter_addr() — internals"
description: "Compiler internals for __elephc_pdo_adapter_addr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 446
---

## `__elephc_pdo_adapter_addr()` — internals

## Where it lives

- **Signature**: [`src/builtins/pointers/elephc_pdo_adapter_addr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/pointers/elephc_pdo_adapter_addr.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/pointers.rs`:235](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/pointers.rs#L235) (`lower_elephc_pdo_adapter_addr`)
- **Function symbol**: `lower_elephc_pdo_adapter_addr()`


### Lowering notes

- Lowers `__elephc_pdo_adapter_addr($kind)` — materializes the GOT address of the
- shared codegen PDO callback adapter selected by the constant `$kind`
- (0 = collation, 1 = scalar user function, 2 = aggregate step, 3 = aggregate
- finalize). The bridge stores this address per registration and calls it back with
- the database-provided arguments, so no bridge extern references a `__rt_*` symbol
- directly.
- The adapter is an external runtime symbol (emitted in the runtime `.text`
- section, gated by `RuntimeFeatures::pdo_udf`), so its address is taken through
- the GOT rather than a same-section page relocation.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_pdo_call_agg_final`
- `__rt_pdo_call_agg_step`
- `__rt_pdo_call_collation`
- `__rt_pdo_call_scalar`

## Signature summary

```php
function __elephc_pdo_adapter_addr(int $kind): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
