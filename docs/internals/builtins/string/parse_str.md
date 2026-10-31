---
title: "parse_str() - internals"
description: "Compiler internals for parse_str(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 977
---

## `parse_str()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/parse_str_prelude.rs`:61](https://github.com/illegalstudio/elephc/blob/main/src/parse_str_prelude.rs#L61) (`parse_str`)
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

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `parse_str()`](../../../php/builtins/string/parse_str.md)
