---
title: "preg_filter() - internals"
description: "Compiler internals for preg_filter(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 778
---

## `preg_filter()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/preg_array_prelude.rs`:482](https://github.com/illegalstudio/elephc/blob/main/src/preg_array_prelude.rs#L482) (`preg_filter`)
- **Function symbol**: `preg_filter()`


### Lowering notes

- Implemented by the compiler-injected preg-array prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_filter(mixed $pattern, mixed $replacement, mixed $subject, int $limit = -1, mixed $count = null): mixed
```

## What the type checker enforces

- **Arity**: takes 3–5 arguments (2 optional).
- **By-reference parameters**: `$count`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_filter.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `preg_filter()`](../../../php/builtins/misc/preg_filter.md)
