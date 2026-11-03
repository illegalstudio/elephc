---
title: "proc_get_status() - internals"
description: "Compiler internals for proc_get_status(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 783
---

## `proc_get_status()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/proc_prelude.rs`:553](https://github.com/illegalstudio/elephc/blob/main/src/proc_prelude.rs#L553) (`proc_get_status`)
- **Function symbol**: `proc_get_status()`


### Lowering notes

- Implemented by the compiler-injected proc prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function proc_get_status(mixed $process): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `proc_get_status()`](../../../php/builtins/misc/proc_get_status.md)
