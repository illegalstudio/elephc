---
title: "proc_terminate() - internals"
description: "Compiler internals for proc_terminate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 786
---

## `proc_terminate()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/proc_prelude.rs`:585](https://github.com/illegalstudio/elephc/blob/main/src/proc_prelude.rs#L585) (`proc_terminate`)
- **Function symbol**: `proc_terminate()`


### Lowering notes

- Implemented by the compiler-injected proc prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function proc_terminate(mixed $process, int $signal = 15): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `proc_terminate()`](../../../php/builtins/misc/proc_terminate.md)
