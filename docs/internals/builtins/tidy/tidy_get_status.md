---
title: "tidy_get_status() - internals"
description: "Compiler internals for tidy_get_status(): lowering path, type checks, and runtime helpers."
---

## `tidy_get_status()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_tidy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_tidy.rs)
- **Lowering**: [`src/tidy_prelude/build/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/tidy_prelude/build/functions.rs) (`tidy_get_status`)
- **Function symbol**: `tidy_get_status()`


### Lowering notes

- Implemented by the compiler-injected tidy prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function tidy_get_status(mixed $tidy): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tidy/tidy_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tidy/tidy_get_status.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `tidy_get_status()`](../../../php/builtins/tidy/tidy_get_status.md)
