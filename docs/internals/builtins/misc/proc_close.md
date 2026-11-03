---
title: "proc_close() - internals"
description: "Compiler internals for proc_close(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 784
---

## `proc_close()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/proc_prelude.rs`:524](https://github.com/illegalstudio/elephc/blob/main/src/proc_prelude.rs#L524) (`proc_close`)
- **Function symbol**: `proc_close()`


### Lowering notes

- Implemented by the compiler-injected proc prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function proc_close(mixed $process): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `proc_close()`](../../../php/builtins/misc/proc_close.md)
