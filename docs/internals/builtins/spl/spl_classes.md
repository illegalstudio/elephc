---
title: "spl_classes() — internals"
description: "Compiler internals for spl_classes(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 334
---

## `spl_classes()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_classes.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_classes.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:206](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L206) (`lower_spl_classes`)
- **Function symbol**: `lower_spl_classes()`


### Lowering notes

- Lowers `spl_classes()` to the static compiler-shipped SPL/core type snapshot.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_itoa`

## Signature summary

```php
function spl_classes(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/spl_classes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_classes.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `spl_classes()`](../../../php/builtins/spl/spl_classes.md)
