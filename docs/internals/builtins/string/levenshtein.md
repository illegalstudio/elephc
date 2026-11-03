---
title: "levenshtein() - internals"
description: "Compiler internals for levenshtein(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 944
---

## `levenshtein()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/backend_gap_prelude.rs`:815](https://github.com/illegalstudio/elephc/blob/main/src/backend_gap_prelude.rs#L815) (`levenshtein`)
- **Function symbol**: `levenshtein()`


### Lowering notes

- Implemented by the compiler-injected backend-gap prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function levenshtein(string $string1, string $string2, int $insertion_cost = 1, int $replacement_cost = 1, int $deletion_cost = 1): int
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/levenshtein.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/levenshtein.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `levenshtein()`](../../../php/builtins/string/levenshtein.md)
