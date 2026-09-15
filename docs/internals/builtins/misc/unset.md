---
title: "unset() - internals"
description: "Compiler internals for unset(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 690
---

## `unset()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:133](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L133) (`lower_unset_builtin`)
- **Function symbol**: `lower_unset_builtin()`


### Lowering notes

- Rejects `unset()` calls that were not converted into direct EIR unbind operations.
- Reaching this lowering means `crate::ir_lower::expr` could not turn the target
- into a slot clear, a hash/array removal, an `offsetUnset()` call, a `__unset()`
- call or a dynamic-property removal, so the message lists the shapes that do lower
- directly. Fixed untyped slots selected by reachable property `unset()` operations
- are widened to boxed `Mixed` and lowered through `PropUnset`, so they do not reach
- this fallback. Packed fields, by-reference slots, and dynamic shapes whose magic
- behavior depends on runtime state remain deliberately unsupported.

## Semantic descriptor

Shared contract with a dedicated compiler language-construct implementation.

## EIR and runtime boundary

_Lowered by a dedicated compiler language-construct path._

## Signature summary

```php
function unset(mixed $var, ...$vars): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$vars`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/unset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/unset.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$vars`.

## Cross-references

- [User reference for `unset()`](../../../php/builtins/misc/unset.md)
