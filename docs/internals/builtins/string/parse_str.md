---
title: "parse_str() - internals"
description: "Compiler internals for parse_str(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1026
---

## `parse_str()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/parse_str_prelude.rs`:453](https://github.com/illegalstudio/elephc/blob/main/src/parse_str_prelude.rs#L453) (`parse_str`)
- **Function symbol**: `parse_str()`


### Lowering notes

- Implemented by the compiler-injected parse_str prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function parse_str(string $string, mixed $result): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$result`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/parse_str.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/parse_str.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`
- **By-reference parameters**: `$result`.

## Cross-references

- [User reference for `parse_str()`](../../../php/builtins/string/parse_str.md)
