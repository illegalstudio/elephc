---
title: "strtok() - internals"
description: "Compiler internals for strtok(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1066
---

## `strtok()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/strtok_prelude.rs`:51](https://github.com/illegalstudio/elephc/blob/main/src/strtok_prelude.rs#L51) (`strtok`)
- **Function symbol**: `strtok()`


### Lowering notes

- Implemented by the compiler-injected strtok prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function strtok(string $string, string $token = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strtok.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtok.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strtok()`](../../../php/builtins/string/strtok.md)
