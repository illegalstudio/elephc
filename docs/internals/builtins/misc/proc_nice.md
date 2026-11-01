---
title: "proc_nice() - internals"
description: "Compiler internals for proc_nice(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 782
---

## `proc_nice()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/proc_prelude.rs`:607](https://github.com/illegalstudio/elephc/blob/main/src/proc_prelude.rs#L607) (`proc_nice`)
- **Function symbol**: `proc_nice()`


### Lowering notes

- Implemented by the compiler-injected proc prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function proc_nice(int $priority): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_nice.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_nice.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `proc_nice()`](../../../php/builtins/misc/proc_nice.md)
