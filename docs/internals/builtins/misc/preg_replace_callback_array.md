---
title: "preg_replace_callback_array() - internals"
description: "Compiler internals for preg_replace_callback_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 780
---

## `preg_replace_callback_array()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/preg_array_prelude.rs`:506](https://github.com/illegalstudio/elephc/blob/main/src/preg_array_prelude.rs#L506) (`preg_replace_callback_array`)
- **Function symbol**: `preg_replace_callback_array()`


### Lowering notes

- Implemented by the compiler-injected preg-array prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_replace_callback_array(array $pattern, mixed $subject, int $limit = -1, mixed $count = null, int $flags = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).
- **By-reference parameters**: `$count`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback_array.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `preg_replace_callback_array()`](../../../php/builtins/misc/preg_replace_callback_array.md)
